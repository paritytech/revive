//! Translation from Yul AST to newyork IR.
//!
//! This module implements the visitor that translates Yul AST into SSA form IR.

use std::collections::{BTreeMap, HashSet};

use num::BigUint;

use revive_yul::lexer::token::lexeme::literal::boolean::Boolean as BooleanLiteral;
use revive_yul::lexer::token::lexeme::literal::integer::Integer as IntegerLiteral;
use revive_yul::lexer::token::lexeme::literal::Literal as LexicalLiteral;
use revive_yul::parser::statement::assignment::Assignment;
use revive_yul::parser::statement::block::Block as YulBlock;
use revive_yul::parser::statement::expression::function_call::name::Name as FunctionName;
use revive_yul::parser::statement::expression::function_call::FunctionCall;
use revive_yul::parser::statement::expression::literal::Literal as YulLiteral;
use revive_yul::parser::statement::expression::Expression as YulExpression;
use revive_yul::parser::statement::for_loop::ForLoop;
use revive_yul::parser::statement::function_definition::FunctionDefinition;
use revive_yul::parser::statement::if_conditional::IfConditional;
use revive_yul::parser::statement::object::Object as YulObject;
use revive_yul::parser::statement::switch::Switch;
use revive_yul::parser::statement::variable_declaration::VariableDeclaration;
use revive_yul::parser::statement::Statement as YulStatement;

use crate::ir::{
    BinaryOperation, BitWidth, Block, CallKind, CreateKind, Expression, Function, FunctionId,
    MemoryRegion, Object, Region, Statement, SwitchCase, Type, UnaryOperation, Value,
};
use crate::ssa::SsaBuilder;

/// Error type for Yul to IR translation.
#[derive(Debug, thiserror::Error)]
pub enum TranslationError {
    /// A variable name was referenced before being declared.
    #[error("Undefined variable: {0}")]
    UndefinedVariable(String),

    /// A function name was referenced before any matching definition was discovered.
    #[error("Undefined function: {0}")]
    UndefinedFunction(String),

    /// A literal could not be parsed (malformed integer, hex, or escape sequence).
    #[error("Invalid literal: {0}")]
    InvalidLiteral(String),

    /// A Yul construct was encountered that this translator does not lower.
    #[error("Unsupported construct: {0}")]
    Unsupported(String),
}

/// Translator from Yul AST to newyork IR.
pub struct YulTranslator {
    /// SSA builder for tracking variables.
    ssa: SsaBuilder,
    /// Function name to ID mapping.
    function_ids: BTreeMap<String, FunctionId>,
    /// Next function ID to allocate.
    next_function_id: u32,
    /// Collected functions.
    functions: BTreeMap<FunctionId, Function>,
    /// Factory dependencies.
    factory_dependencies: HashSet<String>,
    /// Return variable names for the current function being translated.
    /// Used to look up current SSA values when translating `leave` statements.
    current_return_variable_names: Vec<String>,
    /// Stack of loop-carried variable names for the enclosing for loops.
    /// Used to collect current values when translating `break` and `continue`.
    loop_variable_names_stack: Vec<Vec<String>>,
}

impl Default for YulTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl YulTranslator {
    /// Creates a new translator.
    pub fn new() -> Self {
        YulTranslator {
            ssa: SsaBuilder::new(),
            function_ids: BTreeMap::new(),
            next_function_id: 0,
            functions: BTreeMap::new(),
            factory_dependencies: HashSet::new(),
            current_return_variable_names: Vec::new(),
            loop_variable_names_stack: Vec::new(),
        }
    }

    /// Translates a Yul object to IR.
    pub fn translate_object(
        &mut self,
        yul_object: &YulObject,
    ) -> std::result::Result<Object, TranslationError> {
        self.factory_dependencies = yul_object.factory_dependencies.clone();
        self.collect_functions(&yul_object.code.block)?;
        let code = self.translate_block(&yul_object.code.block)?;
        let functions = std::mem::take(&mut self.functions);

        let mut subobjects = Vec::new();
        if let Some(inner_object) = &yul_object.inner_object {
            let mut inner_translator = YulTranslator::new();
            subobjects.push(inner_translator.translate_object(inner_object)?);
        }

        Ok(Object {
            name: yul_object.identifier.clone(),
            code,
            functions,
            subobjects,
            data: BTreeMap::new(),
        })
    }

    /// First pass: walk the AST and pre-allocate `FunctionId`s and parameter `ValueId`s for
    /// every Yul function definition (including those nested inside blocks, if/for/switch).
    /// This lets later passes resolve forward references and reuse parameter IDs across
    /// recursive translation calls.
    fn collect_functions(&mut self, block: &YulBlock) -> std::result::Result<(), TranslationError> {
        for statement in &block.statements {
            if let YulStatement::FunctionDefinition(function_definition) = statement {
                let id = self.allocate_function_id(&function_definition.identifier);
                let mut function = Function::new(id, function_definition.identifier.clone());

                for _parameter in &function_definition.arguments {
                    let parameter_id = self.ssa.fresh_value();
                    function
                        .parameters
                        .push((parameter_id, Type::Int(BitWidth::I256)));
                }
                for _ in &function_definition.result {
                    function.returns.push(Type::Int(BitWidth::I256));
                }

                self.functions.insert(id, function);
            }

            match statement {
                YulStatement::Block(inner) => self.collect_functions(inner)?,
                YulStatement::IfConditional(if_conditional) => {
                    self.collect_functions(&if_conditional.block)?
                }
                YulStatement::ForLoop(for_loop) => {
                    self.collect_functions(&for_loop.initializer)?;
                    self.collect_functions(&for_loop.body)?;
                    self.collect_functions(&for_loop.finalizer)?;
                }
                YulStatement::Switch(switch) => {
                    for case in &switch.cases {
                        self.collect_functions(&case.block)?;
                    }
                    if let Some(default) = &switch.default {
                        self.collect_functions(default)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Allocates a function ID for a name.
    fn allocate_function_id(&mut self, name: &str) -> FunctionId {
        if let Some(&id) = self.function_ids.get(name) {
            return id;
        }
        let id = FunctionId::new(self.next_function_id);
        self.next_function_id += 1;
        self.function_ids.insert(name.to_string(), id);
        id
    }

    /// Looks up a function ID by name.
    fn lookup_function(&self, name: &str) -> Option<FunctionId> {
        self.function_ids.get(name).copied()
    }

    /// Translates a Yul block to an IR block.
    fn translate_block(
        &mut self,
        block: &YulBlock,
    ) -> std::result::Result<Block, TranslationError> {
        let mut ir_block = Block::new();

        for statement in &block.statements {
            let ir_statements = self.translate_statement(statement)?;
            for ir_statement in ir_statements {
                ir_block.push(ir_statement);
            }
        }

        Ok(ir_block)
    }

    /// Translates a Yul block to an IR region.
    fn translate_region(
        &mut self,
        block: &YulBlock,
    ) -> std::result::Result<Region, TranslationError> {
        let mut region = Region::new();

        for statement in &block.statements {
            let ir_statements = self.translate_statement(statement)?;
            for ir_statement in ir_statements {
                region.push(ir_statement);
            }
        }

        Ok(region)
    }

    /// Translates a Yul statement to IR statements.
    fn translate_statement(
        &mut self,
        statement: &YulStatement,
    ) -> std::result::Result<Vec<Statement>, TranslationError> {
        match statement {
            YulStatement::VariableDeclaration(variable_declaration) => {
                self.translate_variable_declaration(variable_declaration)
            }
            YulStatement::Assignment(assignment) => self.translate_assignment(assignment),
            YulStatement::Expression(expression) => self.translate_expression_statement(expression),
            YulStatement::Block(block) => {
                let parent_scope = self.ssa.current_scope().clone();
                self.ssa.enter_scope();
                let region = self.translate_region(block)?;
                let block_scope = self.ssa.exit_scope();

                // Propagate modifications of parent-scope variables back to the parent scope so
                // that assignments inside the block remain visible to enclosing code.
                for (name, value) in &block_scope {
                    if parent_scope.contains_key(name) {
                        self.ssa.define(name, *value);
                    }
                }

                Ok(vec![Statement::Block(region)])
            }
            YulStatement::IfConditional(if_conditional) => self.translate_if(if_conditional),
            YulStatement::Switch(switch) => self.translate_switch(switch),
            YulStatement::ForLoop(for_loop) => self.translate_for_loop(for_loop),
            YulStatement::FunctionDefinition(function_definition) => {
                self.translate_function_definition(function_definition)
            }
            YulStatement::Continue(_) => {
                let values = self.collect_loop_variable_values();
                Ok(vec![Statement::Continue { values }])
            }
            YulStatement::Break(_) => {
                let values = self.collect_loop_variable_values();
                Ok(vec![Statement::Break { values }])
            }
            YulStatement::Leave(_) => {
                let mut return_values = Vec::new();
                for name in &self.current_return_variable_names {
                    if let Some(value) = self.ssa.lookup(name) {
                        return_values.push(value);
                    }
                }
                Ok(vec![Statement::Leave { return_values }])
            }
            // Objects and Code are handled at the top level by `translate_object`.
            YulStatement::Object(_) | YulStatement::Code(_) => Ok(vec![]),
        }
    }

    /// Translates a variable declaration. Tuple destructuring (multiple bindings) requires the
    /// initializer to be a function call that returns the matching number of values; the absence
    /// of an initializer yields zero-initialized i256 bindings.
    fn translate_variable_declaration(
        &mut self,
        variable_declaration: &VariableDeclaration,
    ) -> std::result::Result<Vec<Statement>, TranslationError> {
        let mut statements = Vec::new();

        if let Some(expression) = &variable_declaration.expression {
            let (initializer_statements, initializer_value) =
                self.translate_expression(expression)?;
            statements.extend(initializer_statements);

            if variable_declaration.bindings.len() == 1 {
                let binding = &variable_declaration.bindings[0];
                let value_id = self.ssa.fresh_value();
                let value = Value::new(value_id, Type::Int(BitWidth::I256));
                self.ssa.define(&binding.inner, value);

                statements.push(Statement::Let {
                    bindings: vec![value_id],
                    value: initializer_value,
                });
            } else {
                let mut bindings = Vec::new();
                for binding in &variable_declaration.bindings {
                    let value_id = self.ssa.fresh_value();
                    let value = Value::new(value_id, Type::Int(BitWidth::I256));
                    self.ssa.define(&binding.inner, value);
                    bindings.push(value_id);
                }

                statements.push(Statement::Let {
                    bindings,
                    value: initializer_value,
                });
            }
        } else {
            for binding in &variable_declaration.bindings {
                let value_id = self.ssa.fresh_value();
                let value = Value::new(value_id, Type::Int(BitWidth::I256));
                self.ssa.define(&binding.inner, value);

                statements.push(Statement::Let {
                    bindings: vec![value_id],
                    value: Expression::Literal {
                        value: BigUint::from(0u32),
                        ty: Type::Int(BitWidth::I256),
                    },
                });
            }
        }

        Ok(statements)
    }

    /// Translates an assignment. Multiple bindings imply tuple destructuring of a function call.
    fn translate_assignment(
        &mut self,
        assignment: &Assignment,
    ) -> std::result::Result<Vec<Statement>, TranslationError> {
        let mut statements = Vec::new();

        let (initializer_statements, initializer_value) =
            self.translate_expression(&assignment.initializer)?;
        statements.extend(initializer_statements);

        if assignment.bindings.len() == 1 {
            let binding = &assignment.bindings[0];
            let value_id = self.ssa.fresh_value();
            let value = Value::new(value_id, Type::Int(BitWidth::I256));
            self.ssa.define(&binding.inner, value);

            statements.push(Statement::Let {
                bindings: vec![value_id],
                value: initializer_value,
            });
        } else {
            let mut bindings = Vec::new();
            for binding in &assignment.bindings {
                let value_id = self.ssa.fresh_value();
                let value = Value::new(value_id, Type::Int(BitWidth::I256));
                self.ssa.define(&binding.inner, value);
                bindings.push(value_id);
            }

            statements.push(Statement::Let {
                bindings,
                value: initializer_value,
            });
        }

        Ok(statements)
    }

    /// Translates an expression used as a statement; the expression result is discarded.
    fn translate_expression_statement(
        &mut self,
        expression: &YulExpression,
    ) -> std::result::Result<Vec<Statement>, TranslationError> {
        let (mut statements, ir_expression) = self.translate_expression(expression)?;
        statements.push(Statement::Expression(ir_expression));
        Ok(statements)
    }

    /// Translates an expression, returning any required setup statements and the expression.
    fn translate_expression(
        &mut self,
        expression: &YulExpression,
    ) -> std::result::Result<(Vec<Statement>, Expression), TranslationError> {
        match expression {
            YulExpression::Literal(literal) => {
                let value = self.parse_literal(literal)?;
                Ok((
                    vec![],
                    Expression::Literal {
                        value,
                        ty: Type::Int(BitWidth::I256),
                    },
                ))
            }
            YulExpression::Identifier(identifier) => {
                let value = self
                    .ssa
                    .lookup(&identifier.inner)
                    .ok_or_else(|| TranslationError::UndefinedVariable(identifier.inner.clone()))?;
                Ok((vec![], Expression::Var(value.id)))
            }
            YulExpression::FunctionCall(call) => self.translate_function_call(call),
        }
    }

    /// Translates a function call. The `DataSize`, `DataOffset`, `LoadImmutable`, `SetImmutable`,
    /// and `LinkerSymbol` builtins receive a string literal argument that cannot be evaluated as
    /// an expression, so they are extracted before the generic argument-translation loop runs.
    fn translate_function_call(
        &mut self,
        call: &FunctionCall,
    ) -> std::result::Result<(Vec<Statement>, Expression), TranslationError> {
        match &call.name {
            FunctionName::DataSize => {
                let id = self.extract_string_literal(&call.arguments)?;
                return Ok((vec![], Expression::DataSize { id }));
            }
            FunctionName::DataOffset => {
                let id = self.extract_string_literal(&call.arguments)?;
                return Ok((vec![], Expression::DataOffset { id }));
            }
            FunctionName::LoadImmutable => {
                let key = self.extract_string_literal(&call.arguments)?;
                return Ok((vec![], Expression::LoadImmutable { key }));
            }
            FunctionName::SetImmutable => {
                let key = self.extract_string_literal_at(&call.arguments, 1)?;
                let (mut statements, value_expression) =
                    self.translate_expression(&call.arguments[2])?;
                let value = match value_expression {
                    Expression::Var(id) => Value::new(id, Type::Int(BitWidth::I256)),
                    _ => {
                        let temporary_id = self.ssa.fresh_value();
                        statements.push(Statement::Let {
                            bindings: vec![temporary_id],
                            value: value_expression,
                        });
                        Value::new(temporary_id, Type::Int(BitWidth::I256))
                    }
                };
                statements.push(Statement::SetImmutable { key, value });
                return Ok((
                    statements,
                    Expression::Literal {
                        value: BigUint::from(0u32),
                        ty: Type::Void,
                    },
                ));
            }
            FunctionName::LinkerSymbol => {
                let path = self.extract_string_literal(&call.arguments)?;
                return Ok((vec![], Expression::LinkerSymbol { path }));
            }
            _ => {}
        }

        // Translate arguments in RIGHT-TO-LEFT order per the Yul/EVM spec,
        // then reverse to restore left-to-right order for the call.
        let mut statements = Vec::new();
        let mut arguments = Vec::new();

        for outer_argument_expression in call.arguments.iter().rev() {
            let (argument_statements, argument_expression) =
                self.translate_expression(outer_argument_expression)?;
            statements.extend(argument_statements);

            let argument_value = match argument_expression {
                Expression::Var(id) => Value::new(id, Type::Int(BitWidth::I256)),
                _ => {
                    let temporary_id = self.ssa.fresh_value();
                    statements.push(Statement::Let {
                        bindings: vec![temporary_id],
                        value: argument_expression,
                    });
                    Value::new(temporary_id, Type::Int(BitWidth::I256))
                }
            };
            arguments.push(argument_value);
        }
        arguments.reverse();

        let expression = self.translate_builtin_or_call(&call.name, arguments, &mut statements)?;
        Ok((statements, expression))
    }

    /// Extracts a string literal from the first argument.
    fn extract_string_literal(
        &self,
        arguments: &[YulExpression],
    ) -> std::result::Result<String, TranslationError> {
        self.extract_string_literal_at(arguments, 0)
    }

    /// Extracts a string literal from an argument at a specific index.
    fn extract_string_literal_at(
        &self,
        arguments: &[YulExpression],
        index: usize,
    ) -> std::result::Result<String, TranslationError> {
        if arguments.len() <= index {
            return Err(TranslationError::Unsupported(
                "Missing string literal argument".to_string(),
            ));
        }

        match &arguments[index] {
            YulExpression::Literal(literal) => match &literal.inner {
                LexicalLiteral::String(string) => Ok(string.inner.clone()),
                // Non-string literals are formatted as their decimal string representation.
                _ => Ok(self.parse_literal(literal)?.to_string()),
            },
            _ => Err(TranslationError::Unsupported(
                "Expected literal argument".to_string(),
            )),
        }
    }

    /// Translates a builtin function or user-defined call.
    fn translate_builtin_or_call(
        &mut self,
        name: &FunctionName,
        arguments: Vec<Value>,
        statements: &mut Vec<Statement>,
    ) -> std::result::Result<Expression, TranslationError> {
        match name {
            // Arithmetic operations
            FunctionName::Add => Ok(binary_op(BinaryOperation::Add, &arguments)),
            FunctionName::Sub => Ok(binary_op(BinaryOperation::Sub, &arguments)),
            FunctionName::Mul => Ok(binary_op(BinaryOperation::Mul, &arguments)),
            FunctionName::Div => Ok(binary_op(BinaryOperation::Div, &arguments)),
            FunctionName::Sdiv => Ok(binary_op(BinaryOperation::SDiv, &arguments)),
            FunctionName::Mod => Ok(binary_op(BinaryOperation::Mod, &arguments)),
            FunctionName::Smod => Ok(binary_op(BinaryOperation::SMod, &arguments)),
            FunctionName::Exp => Ok(binary_op(BinaryOperation::Exp, &arguments)),
            FunctionName::AddMod => Ok(ternary_op(BinaryOperation::AddMod, &arguments)),
            FunctionName::MulMod => Ok(ternary_op(BinaryOperation::MulMod, &arguments)),

            // Comparison operations
            FunctionName::Lt => Ok(binary_op(BinaryOperation::Lt, &arguments)),
            FunctionName::Gt => Ok(binary_op(BinaryOperation::Gt, &arguments)),
            FunctionName::Slt => Ok(binary_op(BinaryOperation::Slt, &arguments)),
            FunctionName::Sgt => Ok(binary_op(BinaryOperation::Sgt, &arguments)),
            FunctionName::Eq => Ok(binary_op(BinaryOperation::Eq, &arguments)),
            FunctionName::IsZero => Ok(unary_op(UnaryOperation::IsZero, &arguments)),

            // Bitwise operations
            FunctionName::And => Ok(binary_op(BinaryOperation::And, &arguments)),
            FunctionName::Or => Ok(binary_op(BinaryOperation::Or, &arguments)),
            FunctionName::Xor => Ok(binary_op(BinaryOperation::Xor, &arguments)),
            FunctionName::Not => Ok(unary_op(UnaryOperation::Not, &arguments)),
            FunctionName::Shl => Ok(binary_op(BinaryOperation::Shl, &arguments)),
            FunctionName::Shr => Ok(binary_op(BinaryOperation::Shr, &arguments)),
            FunctionName::Sar => Ok(binary_op(BinaryOperation::Sar, &arguments)),
            FunctionName::Byte => Ok(binary_op(BinaryOperation::Byte, &arguments)),
            FunctionName::SignExtend => Ok(binary_op(BinaryOperation::SignExtend, &arguments)),

            // Memory operations
            FunctionName::MLoad => Ok(Expression::MLoad {
                offset: arguments[0],
                region: MemoryRegion::Unknown,
            }),
            FunctionName::MStore => {
                statements.push(Statement::MStore {
                    offset: arguments[0],
                    value: arguments[1],
                    region: MemoryRegion::Unknown,
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::MStore8 => {
                statements.push(Statement::MStore8 {
                    offset: arguments[0],
                    value: arguments[1],
                    region: MemoryRegion::Unknown,
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::MCopy => {
                statements.push(Statement::MCopy {
                    dest: arguments[0],
                    src: arguments[1],
                    length: arguments[2],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }

            // Storage operations
            FunctionName::SLoad => Ok(Expression::SLoad {
                key: arguments[0],
                static_slot: None,
            }),
            FunctionName::SStore => {
                statements.push(Statement::SStore {
                    key: arguments[0],
                    value: arguments[1],
                    static_slot: None,
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::TLoad => Ok(Expression::TLoad { key: arguments[0] }),
            FunctionName::TStore => {
                statements.push(Statement::TStore {
                    key: arguments[0],
                    value: arguments[1],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }

            // Context getters
            FunctionName::CallDataLoad => Ok(Expression::CallDataLoad {
                offset: arguments[0],
            }),
            FunctionName::CallDataSize => Ok(Expression::CallDataSize),
            FunctionName::CallValue => Ok(Expression::CallValue),
            FunctionName::Caller => Ok(Expression::Caller),
            FunctionName::Origin => Ok(Expression::Origin),
            FunctionName::Address => Ok(Expression::Address),
            FunctionName::Balance => Ok(Expression::Balance {
                address: arguments[0],
            }),
            FunctionName::SelfBalance => Ok(Expression::SelfBalance),
            FunctionName::Gas => Ok(Expression::Gas),
            FunctionName::GasLimit => Ok(Expression::GasLimit),
            FunctionName::GasPrice => Ok(Expression::GasPrice),
            FunctionName::ChainId => Ok(Expression::ChainId),
            FunctionName::Number => Ok(Expression::Number),
            FunctionName::Timestamp => Ok(Expression::Timestamp),
            FunctionName::BlockHash => Ok(Expression::BlockHash {
                number: arguments[0],
            }),
            FunctionName::CoinBase => Ok(Expression::Coinbase),
            FunctionName::Difficulty | FunctionName::Prevrandao => Ok(Expression::Difficulty),
            FunctionName::BaseFee => Ok(Expression::BaseFee),
            FunctionName::BlobBaseFee => Ok(Expression::BlobBaseFee),
            FunctionName::BlobHash => Ok(Expression::BlobHash {
                index: arguments[0],
            }),
            FunctionName::MSize => Ok(Expression::MSize),
            FunctionName::CodeSize => Ok(Expression::CodeSize),
            FunctionName::ExtCodeSize => Ok(Expression::ExtCodeSize {
                address: arguments[0],
            }),
            FunctionName::ExtCodeHash => Ok(Expression::ExtCodeHash {
                address: arguments[0],
            }),
            FunctionName::ReturnDataSize => Ok(Expression::ReturnDataSize),

            // Control flow / termination
            FunctionName::Return => {
                statements.push(Statement::Return {
                    offset: arguments[0],
                    length: arguments[1],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Revert => {
                statements.push(Statement::Revert {
                    offset: arguments[0],
                    length: arguments[1],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Stop => {
                statements.push(Statement::Stop);
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Invalid => {
                statements.push(Statement::Invalid);
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::SelfDestruct => {
                statements.push(Statement::SelfDestruct {
                    address: arguments[0],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }

            // Hashing
            FunctionName::Keccak256 => Ok(Expression::Keccak256 {
                offset: arguments[0],
                length: arguments[1],
            }),

            // External calls
            FunctionName::Call => {
                let result_id = self.ssa.fresh_value();
                statements.push(Statement::ExternalCall {
                    kind: CallKind::Call,
                    gas: arguments[0],
                    address: arguments[1],
                    value: Some(arguments[2]),
                    args_offset: arguments[3],
                    args_length: arguments[4],
                    ret_offset: arguments[5],
                    ret_length: arguments[6],
                    result: result_id,
                });
                Ok(Expression::Var(result_id))
            }
            FunctionName::CallCode => {
                let result_id = self.ssa.fresh_value();
                statements.push(Statement::ExternalCall {
                    kind: CallKind::CallCode,
                    gas: arguments[0],
                    address: arguments[1],
                    value: Some(arguments[2]),
                    args_offset: arguments[3],
                    args_length: arguments[4],
                    ret_offset: arguments[5],
                    ret_length: arguments[6],
                    result: result_id,
                });
                Ok(Expression::Var(result_id))
            }
            FunctionName::DelegateCall => {
                let result_id = self.ssa.fresh_value();
                statements.push(Statement::ExternalCall {
                    kind: CallKind::DelegateCall,
                    gas: arguments[0],
                    address: arguments[1],
                    value: None,
                    args_offset: arguments[2],
                    args_length: arguments[3],
                    ret_offset: arguments[4],
                    ret_length: arguments[5],
                    result: result_id,
                });
                Ok(Expression::Var(result_id))
            }
            FunctionName::StaticCall => {
                let result_id = self.ssa.fresh_value();
                statements.push(Statement::ExternalCall {
                    kind: CallKind::StaticCall,
                    gas: arguments[0],
                    address: arguments[1],
                    value: None,
                    args_offset: arguments[2],
                    args_length: arguments[3],
                    ret_offset: arguments[4],
                    ret_length: arguments[5],
                    result: result_id,
                });
                Ok(Expression::Var(result_id))
            }

            // Contract creation
            FunctionName::Create => {
                let result_id = self.ssa.fresh_value();
                statements.push(Statement::Create {
                    kind: CreateKind::Create,
                    value: arguments[0],
                    offset: arguments[1],
                    length: arguments[2],
                    salt: None,
                    result: result_id,
                });
                Ok(Expression::Var(result_id))
            }
            FunctionName::Create2 => {
                let result_id = self.ssa.fresh_value();
                statements.push(Statement::Create {
                    kind: CreateKind::Create2,
                    value: arguments[0],
                    offset: arguments[1],
                    length: arguments[2],
                    salt: Some(arguments[3]),
                    result: result_id,
                });
                Ok(Expression::Var(result_id))
            }

            // Logging
            FunctionName::Log0 => {
                statements.push(Statement::Log {
                    offset: arguments[0],
                    length: arguments[1],
                    topics: vec![],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Log1 => {
                statements.push(Statement::Log {
                    offset: arguments[0],
                    length: arguments[1],
                    topics: vec![arguments[2]],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Log2 => {
                statements.push(Statement::Log {
                    offset: arguments[0],
                    length: arguments[1],
                    topics: vec![arguments[2], arguments[3]],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Log3 => {
                statements.push(Statement::Log {
                    offset: arguments[0],
                    length: arguments[1],
                    topics: vec![arguments[2], arguments[3], arguments[4]],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Log4 => {
                statements.push(Statement::Log {
                    offset: arguments[0],
                    length: arguments[1],
                    topics: vec![arguments[2], arguments[3], arguments[4], arguments[5]],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }

            // Data operations
            FunctionName::CodeCopy => {
                statements.push(Statement::CodeCopy {
                    dest: arguments[0],
                    offset: arguments[1],
                    length: arguments[2],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::ExtCodeCopy => {
                statements.push(Statement::ExtCodeCopy {
                    address: arguments[0],
                    dest: arguments[1],
                    offset: arguments[2],
                    length: arguments[3],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::ReturnDataCopy => {
                statements.push(Statement::ReturnDataCopy {
                    dest: arguments[0],
                    offset: arguments[1],
                    length: arguments[2],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::CallDataCopy => {
                statements.push(Statement::CallDataCopy {
                    dest: arguments[0],
                    offset: arguments[1],
                    length: arguments[2],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::DataCopy => {
                statements.push(Statement::DataCopy {
                    dest: arguments[0],
                    offset: arguments[1],
                    length: arguments[2],
                });
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }

            // Data size and offset are handled in translate_function_call
            FunctionName::DataSize | FunctionName::DataOffset => {
                unreachable!("DataSize/DataOffset handled in translate_function_call")
            }

            // Special builtins
            FunctionName::Pop => {
                // Pop just discards the value, return a void literal
                Ok(Expression::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::MemoryGuard => {
                // MemoryGuard is an optimization hint, pass through the argument
                if !arguments.is_empty() {
                    Ok(Expression::Var(arguments[0].id))
                } else {
                    Ok(Expression::Literal {
                        value: BigUint::from(0x80u32),
                        ty: Type::Int(BitWidth::I256),
                    })
                }
            }
            FunctionName::LinkerSymbol => {
                // LinkerSymbol is handled in translate_function_call before arguments are evaluated
                unreachable!("LinkerSymbol handled in translate_function_call")
            }

            // PC is not supported on PolkaVM
            FunctionName::Pc => Err(TranslationError::Unsupported("pc".to_string())),

            // CLZ (count leading zeros)
            FunctionName::Clz => Ok(unary_op(UnaryOperation::Clz, &arguments)),

            // Immutables are handled in translate_function_call
            FunctionName::LoadImmutable | FunctionName::SetImmutable => {
                unreachable!("LoadImmutable/SetImmutable handled in translate_function_call")
            }

            // Verbatim
            FunctionName::Verbatim { .. } => {
                Err(TranslationError::Unsupported("verbatim".to_string()))
            }

            // User-defined function call
            FunctionName::UserDefined(name) => {
                let function_id = self
                    .lookup_function(name)
                    .ok_or_else(|| TranslationError::UndefinedFunction(name.clone()))?;
                Ok(Expression::Call {
                    function: function_id,
                    arguments,
                })
            }
        }
    }

    /// Translates an if statement.
    fn translate_if(
        &mut self,
        if_conditional: &IfConditional,
    ) -> std::result::Result<Vec<Statement>, TranslationError> {
        let mut statements = Vec::new();

        // Translate condition
        let (condition_statements, condition_expression) =
            self.translate_expression(&if_conditional.condition)?;
        statements.extend(condition_statements);

        // Create a temporary for the condition if needed
        let condition_value = match condition_expression {
            Expression::Var(id) => Value::new(id, Type::Int(BitWidth::I256)),
            _ => {
                let temporary_id = self.ssa.fresh_value();
                statements.push(Statement::Let {
                    bindings: vec![temporary_id],
                    value: condition_expression,
                });
                Value::new(temporary_id, Type::Int(BitWidth::I256))
            }
        };

        // Save current scope state
        let scope_before = self.ssa.current_scope().clone();

        // Translate then branch
        self.ssa.enter_scope();
        let then_region = self.translate_region(&if_conditional.block)?;
        let then_scope = self.ssa.exit_scope();

        // Yul has no else branch, but we need to handle SSA properly
        // Find variables that were modified in the then branch
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        let mut modified_vars = Vec::new();

        for (name, &then_value) in &then_scope {
            if let Some(&before_value) = scope_before.get(name) {
                if then_value.id != before_value.id {
                    modified_vars.push((name.clone(), before_value, then_value));
                    inputs.push(before_value);
                }
            }
        }

        // Create output values for each modified variable
        for (name, before_value, _then_value) in &modified_vars {
            let output_id = self.ssa.fresh_value();
            outputs.push(output_id);
            self.ssa
                .define(name, Value::new(output_id, before_value.ty));
        }

        // Build the if statement with explicit yields
        let mut then_with_yields = then_region;
        for (_, _, then_value) in &modified_vars {
            then_with_yields.yields.push(*then_value);
        }

        // The else region just yields the original values unchanged
        let else_region = if modified_vars.is_empty() {
            None
        } else {
            let mut else_region = Region::new();
            for (_, before_value, _) in &modified_vars {
                else_region.yields.push(*before_value);
            }
            Some(else_region)
        };

        statements.push(Statement::If {
            condition: condition_value,
            inputs,
            then_region: then_with_yields,
            else_region,
            outputs,
        });

        Ok(statements)
    }

    /// Translates a switch statement.
    fn translate_switch(
        &mut self,
        switch: &Switch,
    ) -> std::result::Result<Vec<Statement>, TranslationError> {
        let mut statements = Vec::new();

        // Translate scrutinee
        let (scrutinee_statements, scrutinee_expression) =
            self.translate_expression(&switch.expression)?;
        statements.extend(scrutinee_statements);

        // Create a temporary for the scrutinee if needed
        let scrutinee_value = match scrutinee_expression {
            Expression::Var(id) => Value::new(id, Type::Int(BitWidth::I256)),
            _ => {
                let temporary_id = self.ssa.fresh_value();
                statements.push(Statement::Let {
                    bindings: vec![temporary_id],
                    value: scrutinee_expression,
                });
                Value::new(temporary_id, Type::Int(BitWidth::I256))
            }
        };

        // Save current scope state
        let scope_before = self.ssa.current_scope().clone();

        // Translate each case ONCE and collect their scopes and regions
        let mut cases = Vec::new();
        let mut all_scopes = Vec::new();

        for case in &switch.cases {
            self.ssa.restore_scope(scope_before.clone());
            self.ssa.enter_scope();
            let case_region = self.translate_region(&case.block)?;
            let case_scope = self.ssa.exit_scope();
            all_scopes.push(case_scope);

            let case_value = self.parse_literal(&case.literal)?;
            cases.push((case_value, case_region));
        }

        // Translate default case ONCE
        let (default_scope, default_region) = if let Some(default_block) = &switch.default {
            self.ssa.restore_scope(scope_before.clone());
            self.ssa.enter_scope();
            let region = self.translate_region(default_block)?;
            let scope = self.ssa.exit_scope();
            (Some(scope), Some(region))
        } else {
            (None, None)
        };

        // Collect all variables that were modified in any branch
        let mut modified_vars: BTreeMap<String, Value> = BTreeMap::new();

        // Check each case scope for modified variables
        for case_scope in &all_scopes {
            for (name, &value) in case_scope {
                if let Some(&before_value) = scope_before.get(name) {
                    if value.id != before_value.id {
                        modified_vars.entry(name.clone()).or_insert(before_value);
                    }
                }
            }
        }

        // Check default scope for modified variables
        if let Some(ref default_scope) = default_scope {
            for (name, &value) in default_scope {
                if let Some(&before_value) = scope_before.get(name) {
                    if value.id != before_value.id {
                        modified_vars.entry(name.clone()).or_insert(before_value);
                    }
                }
            }
        }

        // Get sorted list of modified variable names for deterministic ordering
        let modified_names: Vec<String> = modified_vars.keys().cloned().collect();

        // Create inputs and outputs
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        let mut output_names = Vec::new();

        for name in &modified_names {
            if let Some(&before_value) = modified_vars.get(name) {
                inputs.push(before_value);
                let output_id = self.ssa.fresh_value();
                outputs.push(output_id);
                output_names.push((name.clone(), Value::new(output_id, before_value.ty)));
            }
        }

        // Add yields to already-translated cases (no re-translation needed)
        let mut cases_with_yields = Vec::new();
        for (index, (case_value, mut case_region)) in cases.into_iter().enumerate() {
            let case_scope = &all_scopes[index];
            // Add yields for modified variables
            for name in &modified_names {
                let before_value = modified_vars.get(name).copied().unwrap();
                let value = case_scope.get(name).copied().unwrap_or(before_value);
                case_region.yields.push(value);
            }
            cases_with_yields.push(SwitchCase {
                value: case_value,
                body: case_region,
            });
        }

        // Add yields to already-translated default (no re-translation needed)
        let default_with_yields = if let Some(mut region) = default_region {
            let default_scope = default_scope.as_ref().unwrap();
            // Add yields for modified variables
            for name in &modified_names {
                let before_value = modified_vars.get(name).copied().unwrap();
                let value = default_scope.get(name).copied().unwrap_or(before_value);
                region.yields.push(value);
            }
            Some(region)
        } else if !modified_names.is_empty() {
            // No default branch but we have modified variables - need to yield the originals
            let mut region = Region::new();
            for name in &modified_names {
                let before_value = modified_vars.get(name).copied().unwrap();
                region.yields.push(before_value);
            }
            Some(region)
        } else {
            None
        };

        // Update the scope with output values
        self.ssa.restore_scope(scope_before);
        for (name, value) in output_names {
            self.ssa.define(&name, value);
        }

        statements.push(Statement::Switch {
            scrutinee: scrutinee_value,
            inputs,
            cases: cases_with_yields,
            default: default_with_yields,
            outputs,
        });

        Ok(statements)
    }

    /// Translates a for loop.
    fn translate_for_loop(
        &mut self,
        for_loop: &ForLoop,
    ) -> std::result::Result<Vec<Statement>, TranslationError> {
        let mut statements = Vec::new();

        // Translate initializer
        self.ssa.enter_scope();
        for statement in &for_loop.initializer.statements {
            let initializer_statements = self.translate_statement(statement)?;
            statements.extend(initializer_statements);
        }

        // Identify loop-carried variables (variables defined in initializer)
        let initializer_scope = self.ssa.current_scope().clone();
        let mut loop_variables = Vec::new();
        let mut initializer_values = Vec::new();

        for (name, &value) in &initializer_scope {
            loop_variables.push((name.clone(), self.ssa.fresh_value()));
            initializer_values.push(value);
        }

        // Create new SSA values for loop variables
        for (name, var_id) in &loop_variables {
            let ty = initializer_scope
                .get(name)
                .map(|v| v.ty)
                .unwrap_or_default();
            self.ssa.define(name, Value::new(*var_id, ty));
        }

        // Translate condition - condition_statements will be executed inside the loop header,
        // not before the loop, because they may reference loop_variables
        let (condition_statements, condition_expression) =
            self.translate_expression(&for_loop.condition)?;

        // Translate body in its own scope. The body may modify loop-carried variables.
        // Body yields the current values of ALL loop-carried variables at end of body.
        // These yields are used by the LLVM codegen to create phi nodes at the post-block
        // entry, merging body-end values with continue-site values.
        //
        // Push loop variable names so break/continue can collect current values.
        let loop_variable_names: Vec<String> =
            loop_variables.iter().map(|(n, _)| n.clone()).collect();
        self.loop_variable_names_stack.push(loop_variable_names);

        self.ssa.enter_scope();
        let mut body_region = self.translate_region(&for_loop.body)?;
        let body_scope = self.ssa.exit_scope();

        self.loop_variable_names_stack.pop();

        // Body yields: for each loop-carried variable, yield the body's final value.
        for (name, loop_variable_id) in &loop_variables {
            if let Some(&value) = body_scope.get(name) {
                body_region.yields.push(value);
            } else {
                let ty = initializer_scope
                    .get(name)
                    .map(|v| v.ty)
                    .unwrap_or_default();
                body_region.yields.push(Value::new(*loop_variable_id, ty));
            }
        }

        // Translate post in its own scope. The post receives body outputs as fresh
        // ValueIds (mapped to landing phi values by the LLVM codegen).
        self.ssa.enter_scope();
        let mut post_input_var_ids = Vec::new();
        for (name, _) in loop_variables.iter() {
            let post_var_id = self.ssa.fresh_value();
            post_input_var_ids.push(post_var_id);
            let ty = initializer_scope
                .get(name)
                .map(|v| v.ty)
                .unwrap_or_default();
            self.ssa.define(name, Value::new(post_var_id, ty));
        }

        let mut post_region = self.translate_region(&for_loop.finalizer)?;
        let post_scope = self.ssa.exit_scope();

        // Post yields: the final values of loop-carried variables after the post runs.
        for (name, _) in &loop_variables {
            if let Some(&value) = post_scope.get(name) {
                post_region.yields.push(value);
            }
        }

        // Create output bindings
        let mut outputs = Vec::new();
        let mut output_values = Vec::new();
        for (name, _) in &loop_variables {
            let output_id = self.ssa.fresh_value();
            outputs.push(output_id);
            let ty = initializer_scope
                .get(name)
                .map(|v| v.ty)
                .unwrap_or_default();
            output_values.push((name.clone(), Value::new(output_id, ty)));
        }

        // Exit the loop scope first, then define outputs in the parent scope
        self.ssa.exit_scope();

        // Define output values in the parent scope (which is now current)
        for (name, value) in output_values {
            self.ssa.define(&name, value);
        }

        statements.push(Statement::For {
            initial_values: initializer_values,
            loop_variables: loop_variables.into_iter().map(|(_, id)| id).collect(),
            condition_statements,
            condition: condition_expression,
            body: body_region,
            post_input_variables: post_input_var_ids,
            post: post_region,
            outputs,
        });

        Ok(statements)
    }

    /// Translates a function definition. The function definition does not produce statements in
    /// the containing block — its body is attached to the pre-allocated `Function` entry.
    ///
    /// We save the current scope but keep the SSA counter advanced to ensure globally unique
    /// `ValueId`s. Creating a fresh `SsaBuilder` would restart at ID 0 and collide with the
    /// parameter IDs already allocated by [`Self::collect_functions`].
    fn translate_function_definition(
        &mut self,
        function_definition: &FunctionDefinition,
    ) -> std::result::Result<Vec<Statement>, TranslationError> {
        let function_id = self
            .lookup_function(&function_definition.identifier)
            .ok_or_else(|| {
                TranslationError::UndefinedFunction(function_definition.identifier.clone())
            })?;

        let saved_scope = self.ssa.current_scope().clone();
        self.ssa.restore_scope(BTreeMap::new());

        let function = self.functions.get(&function_id).cloned();
        if let Some(function) = &function {
            for ((parameter_id, _), parameter_identifier) in function
                .parameters
                .iter()
                .zip(&function_definition.arguments)
            {
                self.ssa.define(
                    &parameter_identifier.inner,
                    Value::new(*parameter_id, Type::Int(BitWidth::I256)),
                );
            }
        }

        let mut return_value_ids = Vec::new();
        let saved_return_variable_names = std::mem::take(&mut self.current_return_variable_names);
        for return_identifier in &function_definition.result {
            let return_id = self.ssa.fresh_value();
            self.ssa.define(
                &return_identifier.inner,
                Value::new(return_id, Type::Int(BitWidth::I256)),
            );
            return_value_ids.push(return_id);
            self.current_return_variable_names
                .push(return_identifier.inner.clone());
        }

        let body = self.translate_block(&function_definition.body)?;

        self.current_return_variable_names = saved_return_variable_names;

        let mut final_return_values = Vec::new();
        for return_identifier in &function_definition.result {
            if let Some(value) = self.ssa.lookup(&return_identifier.inner) {
                final_return_values.push(value.id);
            }
        }

        if let Some(function) = self.functions.get_mut(&function_id) {
            function.body = body;
            function.return_values_initial = return_value_ids;
            function.return_values = final_return_values;
            function.size_estimate = estimate_function_size(&function.body);
        }

        self.ssa.restore_scope(saved_scope);

        Ok(vec![])
    }

    /// Collects the current SSA values of the innermost loop's carried variables.
    /// Used by break/continue to carry the right values to the loop's join/post blocks.
    fn collect_loop_variable_values(&self) -> Vec<Value> {
        let Some(variable_names) = self.loop_variable_names_stack.last() else {
            return Vec::new();
        };
        variable_names
            .iter()
            .map(|name| {
                self.ssa
                    .lookup(name)
                    .unwrap_or(Value::new(crate::ir::ValueId::new(0), Type::default()))
            })
            .collect()
    }

    /// Parses a Yul literal to a BigUint.
    fn parse_literal(
        &self,
        literal: &YulLiteral,
    ) -> std::result::Result<BigUint, TranslationError> {
        let inner = &literal.inner;

        match inner {
            LexicalLiteral::Boolean(boolean) => Ok(match boolean {
                BooleanLiteral::True => BigUint::from(1u32),
                BooleanLiteral::False => BigUint::from(0u32),
            }),
            LexicalLiteral::Integer(integer_literal) => match integer_literal {
                IntegerLiteral::Decimal { inner } => BigUint::parse_bytes(inner.as_bytes(), 10)
                    .ok_or_else(|| TranslationError::InvalidLiteral(inner.clone())),
                IntegerLiteral::Hexadecimal { inner } => {
                    let hex = inner
                        .strip_prefix("0x")
                        .or_else(|| inner.strip_prefix("0X"))
                        .unwrap_or(inner);
                    BigUint::parse_bytes(hex.as_bytes(), 16)
                        .ok_or_else(|| TranslationError::InvalidLiteral(inner.clone()))
                }
            },
            // String and hex literals are converted to their byte representation, right-padded
            // to 32 bytes. Escape sequences are processed for regular strings (not hex strings).
            LexicalLiteral::String(string_literal) => {
                let string = &string_literal.inner;
                let mut hex_string = if string_literal.is_hexadecimal {
                    string.clone()
                } else {
                    let mut hex = std::string::String::with_capacity(64);
                    let mut index = 0;
                    let bytes = string.as_bytes();
                    while index < bytes.len() {
                        if bytes[index] == b'\\' {
                            index += 1;
                            if index >= bytes.len() {
                                break;
                            }
                            match bytes[index] {
                                b'x' => {
                                    // \xNN - two hex digit escape
                                    if index + 2 < bytes.len() {
                                        hex.push(bytes[index + 1] as char);
                                        hex.push(bytes[index + 2] as char);
                                    }
                                    index += 3;
                                }
                                b'u' => {
                                    // \uNNNN - unicode escape
                                    if index + 4 < bytes.len() {
                                        let code_point_string = &string[index + 1..index + 5];
                                        if let Ok(codepoint) =
                                            u32::from_str_radix(code_point_string, 16)
                                        {
                                            if let Some(ch) = char::from_u32(codepoint) {
                                                let mut buf = [0u8; 4];
                                                let encoded = ch.encode_utf8(&mut buf);
                                                for byte in encoded.bytes() {
                                                    hex.push_str(&format!("{byte:02x}"));
                                                }
                                            }
                                        }
                                    }
                                    index += 5;
                                }
                                b't' => {
                                    hex.push_str("09");
                                    index += 1;
                                }
                                b'n' => {
                                    hex.push_str("0a");
                                    index += 1;
                                }
                                b'r' => {
                                    hex.push_str("0d");
                                    index += 1;
                                }
                                b'\n' => {
                                    // Line continuation - skip
                                    index += 1;
                                }
                                other => {
                                    hex.push_str(&format!("{other:02x}"));
                                    index += 1;
                                }
                            }
                        } else {
                            hex.push_str(&format!("{:02x}", bytes[index]));
                            index += 1;
                        }
                    }
                    hex
                };

                // Truncate if too long, then right-pad to 32 bytes (64 hex chars)
                if hex_string.len() > 64 {
                    hex_string.truncate(64);
                }
                while hex_string.len() < 64 {
                    hex_string.push('0');
                }

                BigUint::parse_bytes(hex_string.as_bytes(), 16)
                    .ok_or_else(|| TranslationError::InvalidLiteral(string.clone()))
            }
        }
    }
}

/// Creates a binary operation expression.
fn binary_op(op: BinaryOperation, arguments: &[Value]) -> Expression {
    Expression::Binary {
        op,
        lhs: arguments[0],
        rhs: arguments[1],
    }
}

/// Creates a ternary operation expression.
fn ternary_op(op: BinaryOperation, arguments: &[Value]) -> Expression {
    Expression::Ternary {
        op,
        a: arguments[0],
        b: arguments[1],
        n: arguments[2],
    }
}

/// Creates a unary operation expression.
fn unary_op(op: UnaryOperation, arguments: &[Value]) -> Expression {
    Expression::Unary {
        op,
        operand: arguments[0],
    }
}

/// Estimates the size of a function body for inlining decisions.
fn estimate_function_size(block: &Block) -> usize {
    let mut size = 0;
    for statement in &block.statements {
        size += estimate_statement_size(statement);
    }
    size
}

/// Estimates the size of a statement.
fn estimate_statement_size(statement: &Statement) -> usize {
    match statement {
        Statement::Let { .. } => 1,
        Statement::MStore { .. } | Statement::MStore8 { .. } | Statement::MCopy { .. } => 1,
        Statement::SStore { .. } | Statement::TStore { .. } => 1,
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            1 + estimate_region_size(then_region)
                + else_region.as_ref().map_or(0, estimate_region_size)
        }
        Statement::Switch { cases, default, .. } => {
            1 + cases
                .iter()
                .map(|c| estimate_region_size(&c.body))
                .sum::<usize>()
                + default.as_ref().map_or(0, estimate_region_size)
        }
        Statement::For { body, post, .. } => {
            1 + estimate_region_size(body) + estimate_region_size(post)
        }
        Statement::Block(region) => estimate_region_size(region),
        Statement::Expression(_) => 1,
        _ => 1,
    }
}

/// Estimates the size of a region.
fn estimate_region_size(region: &Region) -> usize {
    region.statements.iter().map(estimate_statement_size).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_decimal_literal() {
        let _translator = YulTranslator::new();
        // Would need actual YulLiteral construction for testing
    }
}
