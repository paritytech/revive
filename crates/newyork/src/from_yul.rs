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
    BinOp, BitWidth, Block, CallKind, CreateKind, Expr, Function, FunctionId, MemoryRegion, Object,
    Region, Statement, SwitchCase, Type, UnaryOp, Value,
};
use crate::ssa::SsaBuilder;

/// Error type for Yul to IR translation.
#[derive(Debug, thiserror::Error)]
pub enum TranslationError {
    #[error("Undefined variable: {0}")]
    UndefinedVariable(String),

    #[error("Undefined function: {0}")]
    UndefinedFunction(String),

    #[error("Invalid literal: {0}")]
    InvalidLiteral(String),

    #[error("Unsupported construct: {0}")]
    Unsupported(String),
}

/// Result type for translations.
pub type Result<T> = std::result::Result<T, TranslationError>;

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
    current_return_var_names: Vec<String>,
    /// Stack of loop-carried variable names for the enclosing for loops.
    /// Used to collect current values when translating `break` and `continue`.
    loop_var_names_stack: Vec<Vec<String>>,
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
            current_return_var_names: Vec::new(),
            loop_var_names_stack: Vec::new(),
        }
    }

    /// Translates a Yul object to IR.
    pub fn translate_object(&mut self, yul_object: &YulObject) -> Result<Object> {
        // Store factory dependencies
        self.factory_dependencies = yul_object.factory_dependencies.clone();

        // First pass: collect all function definitions
        self.collect_functions(&yul_object.code.block)?;

        // Translate the main code block
        let code = self.translate_block(&yul_object.code.block)?;

        // Build the functions map
        let functions = std::mem::take(&mut self.functions);

        // Translate subobjects (inner_object for deployed code)
        let mut subobjects = Vec::new();
        if let Some(inner) = &yul_object.inner_object {
            let mut inner_translator = YulTranslator::new();
            let inner_obj = inner_translator.translate_object(inner)?;
            subobjects.push(inner_obj);
        }

        Ok(Object {
            name: yul_object.identifier.clone(),
            code,
            functions,
            subobjects,
            data: BTreeMap::new(),
        })
    }

    /// Collects all function definitions from a block (first pass).
    fn collect_functions(&mut self, block: &YulBlock) -> Result<()> {
        for stmt in &block.statements {
            if let YulStatement::FunctionDefinition(func_def) = stmt {
                let id = self.allocate_function_id(&func_def.identifier);
                let mut function = Function::new(id, func_def.identifier.clone());

                // Set up parameters
                for _param in &func_def.arguments {
                    let param_id = self.ssa.fresh_value();
                    function.params.push((param_id, Type::Int(BitWidth::I256)));
                }

                // Set up return types
                for _ in &func_def.result {
                    function.returns.push(Type::Int(BitWidth::I256));
                }

                self.functions.insert(id, function);
            }

            // Recursively collect from nested blocks
            match stmt {
                YulStatement::Block(inner) => self.collect_functions(inner)?,
                YulStatement::IfConditional(if_cond) => self.collect_functions(&if_cond.block)?,
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
    fn translate_block(&mut self, block: &YulBlock) -> Result<Block> {
        let mut ir_block = Block::new();

        for stmt in &block.statements {
            let ir_stmts = self.translate_statement(stmt)?;
            for s in ir_stmts {
                ir_block.push(s);
            }
        }

        Ok(ir_block)
    }

    /// Translates a Yul block to an IR region.
    fn translate_region(&mut self, block: &YulBlock) -> Result<Region> {
        let mut region = Region::new();

        for stmt in &block.statements {
            let ir_stmts = self.translate_statement(stmt)?;
            for s in ir_stmts {
                region.push(s);
            }
        }

        Ok(region)
    }

    /// Translates a Yul statement to IR statements.
    fn translate_statement(&mut self, stmt: &YulStatement) -> Result<Vec<Statement>> {
        match stmt {
            YulStatement::VariableDeclaration(var_decl) => {
                self.translate_variable_declaration(var_decl)
            }
            YulStatement::Assignment(assign) => self.translate_assignment(assign),
            YulStatement::Expression(expr) => self.translate_expression_statement(expr),
            YulStatement::Block(block) => {
                let parent_scope = self.ssa.current_scope().clone();
                self.ssa.enter_scope();
                let region = self.translate_region(block)?;
                let block_scope = self.ssa.exit_scope();

                // Propagate modifications of parent-scope variables back to parent
                // This ensures assignments inside blocks affect outer-scope variables
                for (name, value) in &block_scope {
                    if parent_scope.contains_key(name) {
                        // Variable from outer scope was modified in the block
                        self.ssa.define(name, *value);
                    }
                }

                Ok(vec![Statement::Block(region)])
            }
            YulStatement::IfConditional(if_cond) => self.translate_if(if_cond),
            YulStatement::Switch(switch) => self.translate_switch(switch),
            YulStatement::ForLoop(for_loop) => self.translate_for_loop(for_loop),
            YulStatement::FunctionDefinition(func_def) => {
                self.translate_function_definition(func_def)
            }
            YulStatement::Continue(_) => {
                let values = self.collect_loop_var_values();
                Ok(vec![Statement::Continue { values }])
            }
            YulStatement::Break(_) => {
                let values = self.collect_loop_var_values();
                Ok(vec![Statement::Break { values }])
            }
            YulStatement::Leave(_) => {
                // Collect current values of return variables
                let mut return_values = Vec::new();
                for name in &self.current_return_var_names {
                    if let Some(value) = self.ssa.lookup(name) {
                        return_values.push(value);
                    }
                }
                Ok(vec![Statement::Leave { return_values }])
            }
            YulStatement::Object(_) | YulStatement::Code(_) => {
                // Objects and Code are handled at the top level
                Ok(vec![])
            }
        }
    }

    /// Translates a variable declaration.
    fn translate_variable_declaration(
        &mut self,
        var_decl: &VariableDeclaration,
    ) -> Result<Vec<Statement>> {
        let mut stmts = Vec::new();

        if let Some(expr) = &var_decl.expression {
            // Translate the initializer expression
            let (init_stmts, init_value) = self.translate_expression(expr)?;
            stmts.extend(init_stmts);

            // Handle multiple bindings (tuple unpacking)
            if var_decl.bindings.len() == 1 {
                let binding = &var_decl.bindings[0];
                let value_id = self.ssa.fresh_value();
                let value = Value::new(value_id, Type::Int(BitWidth::I256));
                self.ssa.define(&binding.inner, value);

                stmts.push(Statement::Let {
                    bindings: vec![value_id],
                    value: init_value,
                });
            } else {
                // Multiple bindings - the expression must be a function call returning multiple values
                let mut bindings = Vec::new();
                for binding in &var_decl.bindings {
                    let value_id = self.ssa.fresh_value();
                    let value = Value::new(value_id, Type::Int(BitWidth::I256));
                    self.ssa.define(&binding.inner, value);
                    bindings.push(value_id);
                }

                stmts.push(Statement::Let {
                    bindings,
                    value: init_value,
                });
            }
        } else {
            // No initializer - create zero-initialized variables
            for binding in &var_decl.bindings {
                let value_id = self.ssa.fresh_value();
                let value = Value::new(value_id, Type::Int(BitWidth::I256));
                self.ssa.define(&binding.inner, value);

                stmts.push(Statement::Let {
                    bindings: vec![value_id],
                    value: Expr::Literal {
                        value: BigUint::from(0u32),
                        ty: Type::Int(BitWidth::I256),
                    },
                });
            }
        }

        Ok(stmts)
    }

    /// Translates an assignment.
    fn translate_assignment(&mut self, assign: &Assignment) -> Result<Vec<Statement>> {
        let mut stmts = Vec::new();

        // Translate the initializer expression
        let (init_stmts, init_value) = self.translate_expression(&assign.initializer)?;
        stmts.extend(init_stmts);

        // Handle multiple bindings (tuple unpacking)
        if assign.bindings.len() == 1 {
            let binding = &assign.bindings[0];
            let value_id = self.ssa.fresh_value();
            let value = Value::new(value_id, Type::Int(BitWidth::I256));
            self.ssa.define(&binding.inner, value);

            stmts.push(Statement::Let {
                bindings: vec![value_id],
                value: init_value,
            });
        } else {
            // Multiple bindings
            let mut bindings = Vec::new();
            for binding in &assign.bindings {
                let value_id = self.ssa.fresh_value();
                let value = Value::new(value_id, Type::Int(BitWidth::I256));
                self.ssa.define(&binding.inner, value);
                bindings.push(value_id);
            }

            stmts.push(Statement::Let {
                bindings,
                value: init_value,
            });
        }

        Ok(stmts)
    }

    /// Translates an expression used as a statement.
    fn translate_expression_statement(&mut self, expr: &YulExpression) -> Result<Vec<Statement>> {
        // The expression result is discarded
        let (mut stmts, ir_expr) = self.translate_expression(expr)?;
        stmts.push(Statement::Expr(ir_expr));
        Ok(stmts)
    }

    /// Translates an expression, returning any required setup statements and the expression.
    fn translate_expression(&mut self, expr: &YulExpression) -> Result<(Vec<Statement>, Expr)> {
        match expr {
            YulExpression::Literal(lit) => {
                let value = self.parse_literal(lit)?;
                Ok((
                    vec![],
                    Expr::Literal {
                        value,
                        ty: Type::Int(BitWidth::I256),
                    },
                ))
            }
            YulExpression::Identifier(ident) => {
                let value = self
                    .ssa
                    .lookup(&ident.inner)
                    .ok_or_else(|| TranslationError::UndefinedVariable(ident.inner.clone()))?;
                Ok((vec![], Expr::Var(value.id)))
            }
            YulExpression::FunctionCall(call) => self.translate_function_call(call),
        }
    }

    /// Translates a function call.
    fn translate_function_call(&mut self, call: &FunctionCall) -> Result<(Vec<Statement>, Expr)> {
        // Handle special cases that need access to the original arguments first
        match &call.name {
            FunctionName::DataSize => {
                // DataSize takes a string literal argument
                let id = self.extract_string_literal(&call.arguments)?;
                return Ok((vec![], Expr::DataSize { id }));
            }
            FunctionName::DataOffset => {
                // DataOffset takes a string literal argument
                let id = self.extract_string_literal(&call.arguments)?;
                return Ok((vec![], Expr::DataOffset { id }));
            }
            FunctionName::LoadImmutable => {
                // LoadImmutable takes a string literal key
                let key = self.extract_string_literal(&call.arguments)?;
                return Ok((vec![], Expr::LoadImmutable { key }));
            }
            FunctionName::SetImmutable => {
                // SetImmutable(base_ptr, key, value) - need the key as string and value
                let key = self.extract_string_literal_at(&call.arguments, 1)?;
                let (value_stmts, value_expr) = self.translate_expression(&call.arguments[2])?;
                let mut stmts = value_stmts;
                let value = match value_expr {
                    Expr::Var(id) => Value::new(id, Type::Int(BitWidth::I256)),
                    _ => {
                        let temp_id = self.ssa.fresh_value();
                        stmts.push(Statement::Let {
                            bindings: vec![temp_id],
                            value: value_expr,
                        });
                        Value::new(temp_id, Type::Int(BitWidth::I256))
                    }
                };
                stmts.push(Statement::SetImmutable { key, value });
                return Ok((
                    stmts,
                    Expr::Literal {
                        value: BigUint::from(0u32),
                        ty: Type::Void,
                    },
                ));
            }
            FunctionName::LinkerSymbol => {
                // LinkerSymbol takes a string literal path argument
                let path = self.extract_string_literal(&call.arguments)?;
                return Ok((vec![], Expr::LinkerSymbol { path }));
            }
            _ => {}
        }

        // Translate arguments in RIGHT-TO-LEFT order per the Yul/EVM spec,
        // then reverse to restore left-to-right order for the call.
        let mut stmts = Vec::new();
        let mut args = Vec::new();

        for arg_expr in call.arguments.iter().rev() {
            let (arg_stmts, arg_expr) = self.translate_expression(arg_expr)?;
            stmts.extend(arg_stmts);

            // Create a temporary for the argument if it's not already a variable reference
            let arg_value = match arg_expr {
                Expr::Var(id) => {
                    // Look up the value in our SSA context to get its type
                    Value::new(id, Type::Int(BitWidth::I256))
                }
                _ => {
                    let temp_id = self.ssa.fresh_value();
                    stmts.push(Statement::Let {
                        bindings: vec![temp_id],
                        value: arg_expr,
                    });
                    Value::new(temp_id, Type::Int(BitWidth::I256))
                }
            };
            args.push(arg_value);
        }
        args.reverse();

        // Translate the function call based on its name
        let expr = self.translate_builtin_or_call(&call.name, args, &mut stmts)?;
        Ok((stmts, expr))
    }

    /// Extracts a string literal from the first argument.
    fn extract_string_literal(&self, args: &[YulExpression]) -> Result<String> {
        self.extract_string_literal_at(args, 0)
    }

    /// Extracts a string literal from an argument at a specific index.
    fn extract_string_literal_at(&self, args: &[YulExpression], index: usize) -> Result<String> {
        if args.len() <= index {
            return Err(TranslationError::Unsupported(
                "Missing string literal argument".to_string(),
            ));
        }

        match &args[index] {
            YulExpression::Literal(lit) => {
                // The literal's inner value may be a string
                match &lit.inner {
                    LexicalLiteral::String(s) => Ok(s.inner.clone()),
                    _ => {
                        // For non-string literals, convert to string representation
                        let value = self.parse_literal(lit)?;
                        Ok(value.to_string())
                    }
                }
            }
            _ => Err(TranslationError::Unsupported(
                "Expected literal argument".to_string(),
            )),
        }
    }

    /// Translates a builtin function or user-defined call.
    fn translate_builtin_or_call(
        &mut self,
        name: &FunctionName,
        args: Vec<Value>,
        stmts: &mut Vec<Statement>,
    ) -> Result<Expr> {
        match name {
            // Arithmetic operations
            FunctionName::Add => Ok(binary_op(BinOp::Add, &args)),
            FunctionName::Sub => Ok(binary_op(BinOp::Sub, &args)),
            FunctionName::Mul => Ok(binary_op(BinOp::Mul, &args)),
            FunctionName::Div => Ok(binary_op(BinOp::Div, &args)),
            FunctionName::Sdiv => Ok(binary_op(BinOp::SDiv, &args)),
            FunctionName::Mod => Ok(binary_op(BinOp::Mod, &args)),
            FunctionName::Smod => Ok(binary_op(BinOp::SMod, &args)),
            FunctionName::Exp => Ok(binary_op(BinOp::Exp, &args)),
            FunctionName::AddMod => Ok(ternary_op(BinOp::AddMod, &args)),
            FunctionName::MulMod => Ok(ternary_op(BinOp::MulMod, &args)),

            // Comparison operations
            FunctionName::Lt => Ok(binary_op(BinOp::Lt, &args)),
            FunctionName::Gt => Ok(binary_op(BinOp::Gt, &args)),
            FunctionName::Slt => Ok(binary_op(BinOp::Slt, &args)),
            FunctionName::Sgt => Ok(binary_op(BinOp::Sgt, &args)),
            FunctionName::Eq => Ok(binary_op(BinOp::Eq, &args)),
            FunctionName::IsZero => Ok(unary_op(UnaryOp::IsZero, &args)),

            // Bitwise operations
            FunctionName::And => Ok(binary_op(BinOp::And, &args)),
            FunctionName::Or => Ok(binary_op(BinOp::Or, &args)),
            FunctionName::Xor => Ok(binary_op(BinOp::Xor, &args)),
            FunctionName::Not => Ok(unary_op(UnaryOp::Not, &args)),
            FunctionName::Shl => Ok(binary_op(BinOp::Shl, &args)),
            FunctionName::Shr => Ok(binary_op(BinOp::Shr, &args)),
            FunctionName::Sar => Ok(binary_op(BinOp::Sar, &args)),
            FunctionName::Byte => Ok(binary_op(BinOp::Byte, &args)),
            FunctionName::SignExtend => Ok(binary_op(BinOp::SignExtend, &args)),

            // Memory operations
            FunctionName::MLoad => Ok(Expr::MLoad {
                offset: args[0],
                region: MemoryRegion::Unknown,
            }),
            FunctionName::MStore => {
                stmts.push(Statement::MStore {
                    offset: args[0],
                    value: args[1],
                    region: MemoryRegion::Unknown,
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::MStore8 => {
                stmts.push(Statement::MStore8 {
                    offset: args[0],
                    value: args[1],
                    region: MemoryRegion::Unknown,
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::MCopy => {
                stmts.push(Statement::MCopy {
                    dest: args[0],
                    src: args[1],
                    length: args[2],
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }

            // Storage operations
            FunctionName::SLoad => Ok(Expr::SLoad {
                key: args[0],
                static_slot: None,
            }),
            FunctionName::SStore => {
                stmts.push(Statement::SStore {
                    key: args[0],
                    value: args[1],
                    static_slot: None,
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::TLoad => Ok(Expr::TLoad { key: args[0] }),
            FunctionName::TStore => {
                stmts.push(Statement::TStore {
                    key: args[0],
                    value: args[1],
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }

            // Context getters
            FunctionName::CallDataLoad => Ok(Expr::CallDataLoad { offset: args[0] }),
            FunctionName::CallDataSize => Ok(Expr::CallDataSize),
            FunctionName::CallValue => Ok(Expr::CallValue),
            FunctionName::Caller => Ok(Expr::Caller),
            FunctionName::Origin => Ok(Expr::Origin),
            FunctionName::Address => Ok(Expr::Address),
            FunctionName::Balance => Ok(Expr::Balance { address: args[0] }),
            FunctionName::SelfBalance => Ok(Expr::SelfBalance),
            FunctionName::Gas => Ok(Expr::Gas),
            FunctionName::GasLimit => Ok(Expr::GasLimit),
            FunctionName::GasPrice => Ok(Expr::GasPrice),
            FunctionName::ChainId => Ok(Expr::ChainId),
            FunctionName::Number => Ok(Expr::Number),
            FunctionName::Timestamp => Ok(Expr::Timestamp),
            FunctionName::BlockHash => Ok(Expr::BlockHash { number: args[0] }),
            FunctionName::CoinBase => Ok(Expr::Coinbase),
            FunctionName::Difficulty | FunctionName::Prevrandao => Ok(Expr::Difficulty),
            FunctionName::BaseFee => Ok(Expr::BaseFee),
            FunctionName::BlobBaseFee => Ok(Expr::BlobBaseFee),
            FunctionName::BlobHash => Ok(Expr::BlobHash { index: args[0] }),
            FunctionName::MSize => Ok(Expr::MSize),
            FunctionName::CodeSize => Ok(Expr::CodeSize),
            FunctionName::ExtCodeSize => Ok(Expr::ExtCodeSize { address: args[0] }),
            FunctionName::ExtCodeHash => Ok(Expr::ExtCodeHash { address: args[0] }),
            FunctionName::ReturnDataSize => Ok(Expr::ReturnDataSize),

            // Control flow / termination
            FunctionName::Return => {
                stmts.push(Statement::Return {
                    offset: args[0],
                    length: args[1],
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Revert => {
                stmts.push(Statement::Revert {
                    offset: args[0],
                    length: args[1],
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Stop => {
                stmts.push(Statement::Stop);
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Invalid => {
                stmts.push(Statement::Invalid);
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::SelfDestruct => {
                stmts.push(Statement::SelfDestruct { address: args[0] });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }

            // Hashing
            FunctionName::Keccak256 => Ok(Expr::Keccak256 {
                offset: args[0],
                length: args[1],
            }),

            // External calls
            FunctionName::Call => {
                let result_id = self.ssa.fresh_value();
                stmts.push(Statement::ExternalCall {
                    kind: CallKind::Call,
                    gas: args[0],
                    address: args[1],
                    value: Some(args[2]),
                    args_offset: args[3],
                    args_length: args[4],
                    ret_offset: args[5],
                    ret_length: args[6],
                    result: result_id,
                });
                Ok(Expr::Var(result_id))
            }
            FunctionName::CallCode => {
                let result_id = self.ssa.fresh_value();
                stmts.push(Statement::ExternalCall {
                    kind: CallKind::CallCode,
                    gas: args[0],
                    address: args[1],
                    value: Some(args[2]),
                    args_offset: args[3],
                    args_length: args[4],
                    ret_offset: args[5],
                    ret_length: args[6],
                    result: result_id,
                });
                Ok(Expr::Var(result_id))
            }
            FunctionName::DelegateCall => {
                let result_id = self.ssa.fresh_value();
                stmts.push(Statement::ExternalCall {
                    kind: CallKind::DelegateCall,
                    gas: args[0],
                    address: args[1],
                    value: None,
                    args_offset: args[2],
                    args_length: args[3],
                    ret_offset: args[4],
                    ret_length: args[5],
                    result: result_id,
                });
                Ok(Expr::Var(result_id))
            }
            FunctionName::StaticCall => {
                let result_id = self.ssa.fresh_value();
                stmts.push(Statement::ExternalCall {
                    kind: CallKind::StaticCall,
                    gas: args[0],
                    address: args[1],
                    value: None,
                    args_offset: args[2],
                    args_length: args[3],
                    ret_offset: args[4],
                    ret_length: args[5],
                    result: result_id,
                });
                Ok(Expr::Var(result_id))
            }

            // Contract creation
            FunctionName::Create => {
                let result_id = self.ssa.fresh_value();
                stmts.push(Statement::Create {
                    kind: CreateKind::Create,
                    value: args[0],
                    offset: args[1],
                    length: args[2],
                    salt: None,
                    result: result_id,
                });
                Ok(Expr::Var(result_id))
            }
            FunctionName::Create2 => {
                let result_id = self.ssa.fresh_value();
                stmts.push(Statement::Create {
                    kind: CreateKind::Create2,
                    value: args[0],
                    offset: args[1],
                    length: args[2],
                    salt: Some(args[3]),
                    result: result_id,
                });
                Ok(Expr::Var(result_id))
            }

            // Logging
            FunctionName::Log0 => {
                stmts.push(Statement::Log {
                    offset: args[0],
                    length: args[1],
                    topics: vec![],
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Log1 => {
                stmts.push(Statement::Log {
                    offset: args[0],
                    length: args[1],
                    topics: vec![args[2]],
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Log2 => {
                stmts.push(Statement::Log {
                    offset: args[0],
                    length: args[1],
                    topics: vec![args[2], args[3]],
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Log3 => {
                stmts.push(Statement::Log {
                    offset: args[0],
                    length: args[1],
                    topics: vec![args[2], args[3], args[4]],
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::Log4 => {
                stmts.push(Statement::Log {
                    offset: args[0],
                    length: args[1],
                    topics: vec![args[2], args[3], args[4], args[5]],
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }

            // Data operations
            FunctionName::CodeCopy => {
                stmts.push(Statement::CodeCopy {
                    dest: args[0],
                    offset: args[1],
                    length: args[2],
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::ExtCodeCopy => {
                stmts.push(Statement::ExtCodeCopy {
                    address: args[0],
                    dest: args[1],
                    offset: args[2],
                    length: args[3],
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::ReturnDataCopy => {
                stmts.push(Statement::ReturnDataCopy {
                    dest: args[0],
                    offset: args[1],
                    length: args[2],
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::CallDataCopy => {
                stmts.push(Statement::CallDataCopy {
                    dest: args[0],
                    offset: args[1],
                    length: args[2],
                });
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::DataCopy => {
                stmts.push(Statement::DataCopy {
                    dest: args[0],
                    offset: args[1],
                    length: args[2],
                });
                Ok(Expr::Literal {
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
                Ok(Expr::Literal {
                    value: BigUint::from(0u32),
                    ty: Type::Void,
                })
            }
            FunctionName::MemoryGuard => {
                // MemoryGuard is an optimization hint, pass through the argument
                if !args.is_empty() {
                    Ok(Expr::Var(args[0].id))
                } else {
                    Ok(Expr::Literal {
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
            FunctionName::Clz => Ok(unary_op(UnaryOp::Clz, &args)),

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
                let func_id = self
                    .lookup_function(name)
                    .ok_or_else(|| TranslationError::UndefinedFunction(name.clone()))?;
                Ok(Expr::Call {
                    function: func_id,
                    args,
                })
            }
        }
    }

    /// Translates an if statement.
    fn translate_if(&mut self, if_cond: &IfConditional) -> Result<Vec<Statement>> {
        let mut stmts = Vec::new();

        // Translate condition
        let (cond_stmts, cond_expr) = self.translate_expression(&if_cond.condition)?;
        stmts.extend(cond_stmts);

        // Create a temporary for the condition if needed
        let cond_value = match cond_expr {
            Expr::Var(id) => Value::new(id, Type::Int(BitWidth::I256)),
            _ => {
                let temp_id = self.ssa.fresh_value();
                stmts.push(Statement::Let {
                    bindings: vec![temp_id],
                    value: cond_expr,
                });
                Value::new(temp_id, Type::Int(BitWidth::I256))
            }
        };

        // Save current scope state
        let scope_before = self.ssa.current_scope().clone();

        // Translate then branch
        self.ssa.enter_scope();
        let then_region = self.translate_region(&if_cond.block)?;
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

        stmts.push(Statement::If {
            condition: cond_value,
            inputs,
            then_region: then_with_yields,
            else_region,
            outputs,
        });

        Ok(stmts)
    }

    /// Translates a switch statement.
    fn translate_switch(&mut self, switch: &Switch) -> Result<Vec<Statement>> {
        let mut stmts = Vec::new();

        // Translate scrutinee
        let (scrut_stmts, scrut_expr) = self.translate_expression(&switch.expression)?;
        stmts.extend(scrut_stmts);

        // Create a temporary for the scrutinee if needed
        let scrut_value = match scrut_expr {
            Expr::Var(id) => Value::new(id, Type::Int(BitWidth::I256)),
            _ => {
                let temp_id = self.ssa.fresh_value();
                stmts.push(Statement::Let {
                    bindings: vec![temp_id],
                    value: scrut_expr,
                });
                Value::new(temp_id, Type::Int(BitWidth::I256))
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
        for (idx, (case_value, mut case_region)) in cases.into_iter().enumerate() {
            let case_scope = &all_scopes[idx];
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

        stmts.push(Statement::Switch {
            scrutinee: scrut_value,
            inputs,
            cases: cases_with_yields,
            default: default_with_yields,
            outputs,
        });

        Ok(stmts)
    }

    /// Translates a for loop.
    fn translate_for_loop(&mut self, for_loop: &ForLoop) -> Result<Vec<Statement>> {
        let mut stmts = Vec::new();

        // Translate initializer
        self.ssa.enter_scope();
        for stmt in &for_loop.initializer.statements {
            let init_stmts = self.translate_statement(stmt)?;
            stmts.extend(init_stmts);
        }

        // Identify loop-carried variables (variables defined in initializer)
        let init_scope = self.ssa.current_scope().clone();
        let mut loop_vars = Vec::new();
        let mut init_values = Vec::new();

        for (name, &value) in &init_scope {
            loop_vars.push((name.clone(), self.ssa.fresh_value()));
            init_values.push(value);
        }

        // Create new SSA values for loop variables
        for (name, var_id) in &loop_vars {
            let ty = init_scope.get(name).map(|v| v.ty).unwrap_or_default();
            self.ssa.define(name, Value::new(*var_id, ty));
        }

        // Translate condition - condition_stmts will be executed inside the loop header,
        // not before the loop, because they may reference loop_vars
        let (condition_stmts, cond_expr) = self.translate_expression(&for_loop.condition)?;

        // Translate body in its own scope. The body may modify loop-carried variables.
        // Body yields the current values of ALL loop-carried variables at end of body.
        // These yields are used by the LLVM codegen to create phi nodes at the post-block
        // entry, merging body-end values with continue-site values.
        //
        // Push loop var names so break/continue can collect current values.
        let loop_var_names: Vec<String> = loop_vars.iter().map(|(n, _)| n.clone()).collect();
        self.loop_var_names_stack.push(loop_var_names);

        self.ssa.enter_scope();
        let mut body_region = self.translate_region(&for_loop.body)?;
        let body_scope = self.ssa.exit_scope();

        self.loop_var_names_stack.pop();

        // Body yields: for each loop-carried variable, yield the body's final value.
        for (name, loop_var_id) in &loop_vars {
            if let Some(&value) = body_scope.get(name) {
                body_region.yields.push(value);
            } else {
                let ty = init_scope.get(name).map(|v| v.ty).unwrap_or_default();
                body_region.yields.push(Value::new(*loop_var_id, ty));
            }
        }

        // Translate post in its own scope. The post receives body outputs as fresh
        // ValueIds (mapped to landing phi values by the LLVM codegen).
        self.ssa.enter_scope();
        let mut post_input_var_ids = Vec::new();
        for (name, _) in loop_vars.iter() {
            let post_var_id = self.ssa.fresh_value();
            post_input_var_ids.push(post_var_id);
            let ty = init_scope.get(name).map(|v| v.ty).unwrap_or_default();
            self.ssa.define(name, Value::new(post_var_id, ty));
        }

        let mut post_region = self.translate_region(&for_loop.finalizer)?;
        let post_scope = self.ssa.exit_scope();

        // Post yields: the final values of loop-carried variables after the post runs.
        for (name, _) in &loop_vars {
            if let Some(&value) = post_scope.get(name) {
                post_region.yields.push(value);
            }
        }

        // Create output bindings
        let mut outputs = Vec::new();
        let mut output_values = Vec::new();
        for (name, _) in &loop_vars {
            let output_id = self.ssa.fresh_value();
            outputs.push(output_id);
            let ty = init_scope.get(name).map(|v| v.ty).unwrap_or_default();
            output_values.push((name.clone(), Value::new(output_id, ty)));
        }

        // Exit the loop scope first, then define outputs in the parent scope
        self.ssa.exit_scope();

        // Define output values in the parent scope (which is now current)
        for (name, value) in output_values {
            self.ssa.define(&name, value);
        }

        stmts.push(Statement::For {
            init_values,
            loop_vars: loop_vars.into_iter().map(|(_, id)| id).collect(),
            condition_stmts,
            condition: cond_expr,
            body: body_region,
            post_input_vars: post_input_var_ids,
            post: post_region,
            outputs,
        });

        Ok(stmts)
    }

    /// Translates a function definition.
    fn translate_function_definition(
        &mut self,
        func_def: &FunctionDefinition,
    ) -> Result<Vec<Statement>> {
        // Get the function ID
        let func_id = self
            .lookup_function(&func_def.identifier)
            .ok_or_else(|| TranslationError::UndefinedFunction(func_def.identifier.clone()))?;

        // Save the current scope but keep the SSA counter to ensure globally unique IDs.
        // This is critical: if we create a fresh SsaBuilder, it starts from ID 0 which
        // conflicts with parameter IDs that were allocated in collect_functions.
        let saved_scope = self.ssa.current_scope().clone();
        self.ssa.restore_scope(BTreeMap::new());

        // Define parameters in the function scope
        let func = self.functions.get(&func_id).cloned();
        if let Some(func) = &func {
            for ((param_id, _ty), param_ident) in func.params.iter().zip(&func_def.arguments) {
                self.ssa.define(
                    &param_ident.inner,
                    Value::new(*param_id, Type::Int(BitWidth::I256)),
                );
            }
        }

        // Define return variables and track their IDs and names
        let mut return_value_ids = Vec::new();
        let saved_return_var_names = std::mem::take(&mut self.current_return_var_names);
        for ret_ident in &func_def.result {
            let ret_id = self.ssa.fresh_value();
            self.ssa.define(
                &ret_ident.inner,
                Value::new(ret_id, Type::Int(BitWidth::I256)),
            );
            return_value_ids.push(ret_id);
            self.current_return_var_names.push(ret_ident.inner.clone());
        }

        // Translate the function body
        let body = self.translate_block(&func_def.body)?;

        // Restore the return var names for outer function (if nested)
        self.current_return_var_names = saved_return_var_names;

        // Collect final values of return variables after body execution
        let mut final_return_values = Vec::new();
        for ret_ident in &func_def.result {
            if let Some(value) = self.ssa.lookup(&ret_ident.inner) {
                final_return_values.push(value.id);
            }
        }

        // Update the function with its body and return value IDs
        if let Some(func) = self.functions.get_mut(&func_id) {
            func.body = body;
            func.return_values_initial = return_value_ids;
            func.return_values = final_return_values;
            func.size_estimate = estimate_function_size(&func.body);
        }

        // Restore the original scope (keeps the counter advanced)
        self.ssa.restore_scope(saved_scope);

        // Function definitions don't produce statements in the containing block
        Ok(vec![])
    }

    /// Collects the current SSA values of the innermost loop's carried variables.
    /// Used by break/continue to carry the right values to the loop's join/post blocks.
    fn collect_loop_var_values(&self) -> Vec<Value> {
        let Some(var_names) = self.loop_var_names_stack.last() else {
            return Vec::new();
        };
        var_names
            .iter()
            .map(|name| {
                self.ssa
                    .lookup(name)
                    .unwrap_or(Value::new(crate::ir::ValueId::new(0), Type::default()))
            })
            .collect()
    }

    /// Parses a Yul literal to a BigUint.
    fn parse_literal(&self, lit: &YulLiteral) -> Result<BigUint> {
        let inner = &lit.inner;

        // Handle different literal types
        match inner {
            LexicalLiteral::Boolean(b) => Ok(match b {
                BooleanLiteral::True => BigUint::from(1u32),
                BooleanLiteral::False => BigUint::from(0u32),
            }),
            LexicalLiteral::Integer(int_lit) => {
                // Parse the integer literal
                match int_lit {
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
                }
            }
            LexicalLiteral::String(str_lit) => {
                // String/hex literals are converted to their byte representation,
                // right-padded to 32 bytes. Escape sequences are processed for
                // regular strings (not hex strings).
                let string = &str_lit.inner;
                let mut hex_string = if str_lit.is_hexadecimal {
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
                                        let cp_str = &string[index + 1..index + 5];
                                        if let Ok(codepoint) = u32::from_str_radix(cp_str, 16) {
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
fn binary_op(op: BinOp, args: &[Value]) -> Expr {
    Expr::Binary {
        op,
        lhs: args[0],
        rhs: args[1],
    }
}

/// Creates a ternary operation expression.
fn ternary_op(op: BinOp, args: &[Value]) -> Expr {
    Expr::Ternary {
        op,
        a: args[0],
        b: args[1],
        n: args[2],
    }
}

/// Creates a unary operation expression.
fn unary_op(op: UnaryOp, args: &[Value]) -> Expr {
    Expr::Unary {
        op,
        operand: args[0],
    }
}

/// Estimates the size of a function body for inlining decisions.
fn estimate_function_size(block: &Block) -> usize {
    let mut size = 0;
    for stmt in &block.statements {
        size += estimate_statement_size(stmt);
    }
    size
}

/// Estimates the size of a statement.
fn estimate_statement_size(stmt: &Statement) -> usize {
    match stmt {
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
        Statement::Expr(_) => 1,
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
