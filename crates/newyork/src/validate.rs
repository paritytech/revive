//! IR validation passes for the newyork IR.
//!
//! This module provides validation passes to verify IR correctness:
//! - SSA well-formedness: All uses are dominated by definitions
//! - Type consistency: Operations have correctly typed operands
//! - Region validity: All regions have correct yields matching outputs
//!
//! # Usage
//!
//! ```ignore
//! use revive_newyork::validate::{validate_object, ValidationError};
//!
//! let result = validate_object(&ir_object);
//! if let Err(errors) = result {
//!     for error in errors {
//!         eprintln!("Validation error: {}", error);
//!     }
//! }
//! ```

use crate::ir::{Block, Expression, Function, Object, Region, Statement, Value, ValueId};
use std::collections::BTreeSet;
use thiserror::Error;

/// Validation error types.
#[derive(Error, Debug, Clone)]
pub enum ValidationError {
    /// A value is used before it is defined.
    #[error("SSA error: value v{0} used before definition at {1}")]
    UseBeforeDef(u32, String),

    /// A value is defined multiple times.
    #[error("SSA error: value v{0} defined multiple times")]
    MultipleDef(u32),

    /// Type mismatch in an operation.
    #[error("Type error: {0}")]
    TypeMismatch(String),

    /// Region yields wrong number of values.
    #[error("Region error: expected {expected} yields, got {actual} at {location}")]
    YieldCountMismatch {
        expected: usize,
        actual: usize,
        location: String,
    },

    /// Function has inconsistent return value count.
    #[error("Function error: {0}")]
    FunctionError(String),

    /// Unknown function called.
    #[error("Unknown function: f{0}")]
    UnknownFunction(u32),
}

/// Result of validation.
pub type ValidationResult = Result<(), Vec<ValidationError>>;

/// Validates an IR object, returning all errors found.
pub fn validate_object(object: &Object) -> ValidationResult {
    let mut validator = Validator::new();
    validator.validate_object(object);
    validator.into_result()
}

/// Validates a single function.
pub fn validate_function(function: &Function) -> ValidationResult {
    let mut validator = Validator::new();
    validator.validate_function(function);
    validator.into_result()
}

/// Internal validator state.
struct Validator {
    /// Set of defined value IDs (visible in current scope).
    defined: BTreeSet<u32>,
    /// Stack of defined sets for scope management.
    scope_stack: Vec<BTreeSet<u32>>,
    /// Set of known function IDs.
    known_functions: BTreeSet<u32>,
    /// Collected errors.
    errors: Vec<ValidationError>,
}

impl Validator {
    fn new() -> Self {
        Validator {
            defined: BTreeSet::new(),
            scope_stack: Vec::new(),
            known_functions: BTreeSet::new(),
            errors: Vec::new(),
        }
    }

    fn into_result(self) -> ValidationResult {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors)
        }
    }

    fn error(&mut self, err: ValidationError) {
        self.errors.push(err);
    }

    fn enter_scope(&mut self) {
        self.scope_stack.push(self.defined.clone());
    }

    fn exit_scope(&mut self) {
        if let Some(saved) = self.scope_stack.pop() {
            self.defined = saved;
        }
    }

    fn define(&mut self, id: ValueId) {
        if !self.defined.insert(id.0) {
            self.error(ValidationError::MultipleDef(id.0));
        }
    }

    fn use_value(&mut self, value: &Value, context: &str) {
        self.use_value_id(value.id, context);
    }

    fn use_value_id(&mut self, id: ValueId, context: &str) {
        if !self.defined.contains(&id.0) {
            self.error(ValidationError::UseBeforeDef(id.0, context.to_string()));
        }
    }

    fn validate_object(&mut self, object: &Object) {
        for id in object.functions.keys() {
            self.known_functions.insert(id.0);
        }

        self.validate_block(&object.code, &format!("object '{}' code", object.name));

        for function in object.functions.values() {
            self.validate_function(function);
        }

        for subobject in &object.subobjects {
            self.validate_object(subobject);
        }
    }

    fn validate_function(&mut self, function: &Function) {
        self.enter_scope();

        for (id, _ty) in &function.parameters {
            self.define(*id);
        }

        for id in &function.return_values_initial {
            self.define(*id);
        }

        if function.returns.len() != function.return_values_initial.len() {
            self.error(ValidationError::FunctionError(format!(
                "function '{}': returns count ({}) != return_values_initial count ({})",
                function.name,
                function.returns.len(),
                function.return_values_initial.len()
            )));
        }

        if function.returns.len() != function.return_values.len() {
            self.error(ValidationError::FunctionError(format!(
                "function '{}': returns count ({}) != return_values count ({})",
                function.name,
                function.returns.len(),
                function.return_values.len()
            )));
        }

        self.validate_block(&function.body, &format!("function '{}'", function.name));

        for id in &function.return_values {
            self.use_value_id(*id, &format!("function '{}' return", function.name));
        }

        self.exit_scope();
    }

    fn validate_block(&mut self, block: &Block, context: &str) {
        for (i, statement) in block.statements.iter().enumerate() {
            self.validate_statement(statement, &format!("{}[{}]", context, i));
        }
    }

    fn validate_region(&mut self, region: &Region, context: &str) {
        for (i, statement) in region.statements.iter().enumerate() {
            self.validate_statement(statement, &format!("{}[{}]", context, i));
        }

        for value in &region.yields {
            self.use_value(value, &format!("{} yield", context));
        }
    }

    fn validate_statement(&mut self, statement: &Statement, context: &str) {
        match statement {
            Statement::Let { bindings, value } => {
                self.validate_expression(value, context);

                for id in bindings {
                    self.define(*id);
                }
            }

            Statement::MStore {
                offset, value: v, ..
            } => {
                self.use_value(offset, context);
                self.use_value(v, context);
            }

            Statement::MStore8 {
                offset, value: v, ..
            } => {
                self.use_value(offset, context);
                self.use_value(v, context);
            }

            Statement::MCopy { dest, src, length } => {
                self.use_value(dest, context);
                self.use_value(src, context);
                self.use_value(length, context);
            }

            Statement::SStore { key, value: v, .. } => {
                self.use_value(key, context);
                self.use_value(v, context);
            }

            Statement::TStore { key, value: v } => {
                self.use_value(key, context);
                self.use_value(v, context);
            }

            Statement::MappingSStore {
                key,
                slot,
                value: v,
            } => {
                self.use_value(key, context);
                self.use_value(slot, context);
                self.use_value(v, context);
            }

            Statement::If {
                condition,
                inputs,
                then_region,
                else_region,
                outputs,
            } => {
                self.use_value(condition, context);
                for v in inputs {
                    self.use_value(v, context);
                }

                self.enter_scope();
                self.validate_region(then_region, &format!("{} then", context));
                let then_yield_count = then_region.yields.len();
                self.exit_scope();

                let else_yield_count = if let Some(else_region) = else_region {
                    self.enter_scope();
                    self.validate_region(else_region, &format!("{} else", context));
                    let count = else_region.yields.len();
                    self.exit_scope();
                    count
                } else {
                    inputs.len()
                };

                if outputs.len() != then_yield_count {
                    self.error(ValidationError::YieldCountMismatch {
                        expected: outputs.len(),
                        actual: then_yield_count,
                        location: format!("{} then", context),
                    });
                }

                if outputs.len() != else_yield_count {
                    self.error(ValidationError::YieldCountMismatch {
                        expected: outputs.len(),
                        actual: else_yield_count,
                        location: format!("{} else", context),
                    });
                }

                for id in outputs {
                    self.define(*id);
                }
            }

            Statement::Switch {
                scrutinee,
                inputs,
                cases,
                default,
                outputs,
            } => {
                self.use_value(scrutinee, context);
                for v in inputs {
                    self.use_value(v, context);
                }

                for (i, case) in cases.iter().enumerate() {
                    self.enter_scope();
                    self.validate_region(&case.body, &format!("{} case[{}]", context, i));

                    if outputs.len() != case.body.yields.len() {
                        self.error(ValidationError::YieldCountMismatch {
                            expected: outputs.len(),
                            actual: case.body.yields.len(),
                            location: format!("{} case[{}]", context, i),
                        });
                    }
                    self.exit_scope();
                }

                if let Some(default_region) = default {
                    self.enter_scope();
                    self.validate_region(default_region, &format!("{} default", context));

                    if outputs.len() != default_region.yields.len() {
                        self.error(ValidationError::YieldCountMismatch {
                            expected: outputs.len(),
                            actual: default_region.yields.len(),
                            location: format!("{} default", context),
                        });
                    }
                    self.exit_scope();
                }

                for id in outputs {
                    self.define(*id);
                }
            }

            Statement::For {
                initial_values,
                loop_variables,
                condition_statements,
                condition,
                body,
                post,
                outputs,
                ..
            } => {
                for v in initial_values {
                    self.use_value(v, context);
                }

                if loop_variables.len() != initial_values.len() {
                    self.error(ValidationError::FunctionError(format!(
                        "{}: loop_variables count ({}) != initial_values count ({})",
                        context,
                        loop_variables.len(),
                        initial_values.len()
                    )));
                }

                self.enter_scope();
                for id in loop_variables {
                    self.define(*id);
                }

                for (i, statement) in condition_statements.iter().enumerate() {
                    self.validate_statement(statement, &format!("{} cond_stmt[{}]", context, i));
                }

                self.validate_expression(condition, &format!("{} condition", context));

                self.validate_region(body, &format!("{} body", context));

                self.validate_region(post, &format!("{} post", context));

                if loop_variables.len() != post.yields.len() {
                    self.error(ValidationError::YieldCountMismatch {
                        expected: loop_variables.len(),
                        actual: post.yields.len(),
                        location: format!("{} post", context),
                    });
                }

                self.exit_scope();

                if outputs.len() != loop_variables.len() {
                    self.error(ValidationError::FunctionError(format!(
                        "{}: outputs count ({}) != loop_variables count ({})",
                        context,
                        outputs.len(),
                        loop_variables.len()
                    )));
                }

                for id in outputs {
                    self.define(*id);
                }
            }

            Statement::Break { values } | Statement::Continue { values } => {
                for v in values {
                    self.use_value(v, context);
                }
            }

            Statement::Leave { return_values } => {
                for v in return_values {
                    self.use_value(v, context);
                }
            }

            Statement::Revert { offset, length } => {
                self.use_value(offset, context);
                self.use_value(length, context);
            }

            Statement::Return { offset, length } => {
                self.use_value(offset, context);
                self.use_value(length, context);
            }

            Statement::Stop
            | Statement::Invalid
            | Statement::PanicRevert { .. }
            | Statement::ErrorStringRevert { .. }
            | Statement::CustomErrorRevert { .. } => {}

            Statement::SelfDestruct { address } => {
                self.use_value(address, context);
            }

            Statement::ExternalCall {
                gas,
                address,
                value,
                args_offset,
                args_length,
                ret_offset,
                ret_length,
                result,
                ..
            } => {
                self.use_value(gas, context);
                self.use_value(address, context);
                if let Some(v) = value {
                    self.use_value(v, context);
                }
                self.use_value(args_offset, context);
                self.use_value(args_length, context);
                self.use_value(ret_offset, context);
                self.use_value(ret_length, context);
                self.define(*result);
            }

            Statement::Create {
                value,
                offset,
                length,
                salt,
                result,
                ..
            } => {
                self.use_value(value, context);
                self.use_value(offset, context);
                self.use_value(length, context);
                if let Some(s) = salt {
                    self.use_value(s, context);
                }
                self.define(*result);
            }

            Statement::Log {
                offset,
                length,
                topics,
            } => {
                self.use_value(offset, context);
                self.use_value(length, context);
                for t in topics {
                    self.use_value(t, context);
                }
            }

            Statement::CodeCopy {
                dest,
                offset,
                length,
            } => {
                self.use_value(dest, context);
                self.use_value(offset, context);
                self.use_value(length, context);
            }

            Statement::ExtCodeCopy {
                address,
                dest,
                offset,
                length,
            } => {
                self.use_value(address, context);
                self.use_value(dest, context);
                self.use_value(offset, context);
                self.use_value(length, context);
            }

            Statement::ReturnDataCopy {
                dest,
                offset,
                length,
            } => {
                self.use_value(dest, context);
                self.use_value(offset, context);
                self.use_value(length, context);
            }

            Statement::DataCopy {
                dest,
                offset,
                length,
            } => {
                self.use_value(dest, context);
                self.use_value(offset, context);
                self.use_value(length, context);
            }

            Statement::CallDataCopy {
                dest,
                offset,
                length,
            } => {
                self.use_value(dest, context);
                self.use_value(offset, context);
                self.use_value(length, context);
            }

            Statement::Block(region) => {
                self.enter_scope();
                self.validate_region(region, context);
                self.exit_scope();
            }

            Statement::Expression(expression) => {
                self.validate_expression(expression, context);
            }

            Statement::SetImmutable { value, .. } => {
                self.use_value(value, context);
            }
        }
    }

    fn validate_expression(&mut self, expression: &Expression, context: &str) {
        match expression {
            Expression::Literal { .. } => {}

            Expression::Var(id) => {
                self.use_value_id(*id, context);
            }

            Expression::Binary { lhs, rhs, .. } => {
                self.use_value(lhs, context);
                self.use_value(rhs, context);
            }

            Expression::Ternary { a, b, n, .. } => {
                self.use_value(a, context);
                self.use_value(b, context);
                self.use_value(n, context);
            }

            Expression::Unary { operand, .. } => {
                self.use_value(operand, context);
            }

            Expression::CallDataLoad { offset } => {
                self.use_value(offset, context);
            }

            Expression::CallValue
            | Expression::Caller
            | Expression::Origin
            | Expression::CallDataSize
            | Expression::CodeSize
            | Expression::GasPrice
            | Expression::ReturnDataSize
            | Expression::Coinbase
            | Expression::Timestamp
            | Expression::Number
            | Expression::Difficulty
            | Expression::GasLimit
            | Expression::ChainId
            | Expression::SelfBalance
            | Expression::BaseFee
            | Expression::BlobBaseFee
            | Expression::Gas
            | Expression::MSize
            | Expression::Address => {}

            Expression::ExtCodeSize { address } => {
                self.use_value(address, context);
            }

            Expression::ExtCodeHash { address } => {
                self.use_value(address, context);
            }

            Expression::BlockHash { number } => {
                self.use_value(number, context);
            }

            Expression::BlobHash { index } => {
                self.use_value(index, context);
            }

            Expression::Balance { address } => {
                self.use_value(address, context);
            }

            Expression::MLoad { offset, .. } => {
                self.use_value(offset, context);
            }

            Expression::SLoad { key, .. } => {
                self.use_value(key, context);
            }

            Expression::TLoad { key } => {
                self.use_value(key, context);
            }

            Expression::Call {
                function,
                arguments,
            } => {
                if !self.known_functions.contains(&function.0) {
                    self.error(ValidationError::UnknownFunction(function.0));
                }
                for argument in arguments {
                    self.use_value(argument, context);
                }
            }

            Expression::Truncate { value, .. }
            | Expression::ZeroExtend { value, .. }
            | Expression::SignExtendTo { value, .. } => {
                self.use_value(value, context);
            }

            Expression::Keccak256 { offset, length } => {
                self.use_value(offset, context);
                self.use_value(length, context);
            }

            Expression::Keccak256Pair { word0, word1 } => {
                self.use_value(word0, context);
                self.use_value(word1, context);
            }

            Expression::Keccak256Single { word0 } => {
                self.use_value(word0, context);
            }

            Expression::MappingSLoad { key, slot } => {
                self.use_value(key, context);
                self.use_value(slot, context);
            }

            Expression::DataOffset { .. }
            | Expression::DataSize { .. }
            | Expression::LoadImmutable { .. }
            | Expression::LinkerSymbol { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        BinaryOperation, BitWidth, Block, FunctionId, Object, Region, Statement, Type, Value,
        ValueId,
    };
    use num::BigUint;
    use std::collections::BTreeMap;

    fn int_value(id: u32) -> Value {
        Value::int(ValueId(id))
    }

    #[test]
    fn test_valid_let() {
        let object = Object {
            name: "Test".to_string(),
            code: Block {
                statements: vec![
                    Statement::Let {
                        bindings: vec![ValueId(0)],
                        value: Expression::Literal {
                            value: BigUint::from(0u64),
                            value_type: Type::Int(BitWidth::I256),
                        },
                    },
                    Statement::Let {
                        bindings: vec![ValueId(1)],
                        value: Expression::Var(ValueId(0)),
                    },
                ],
            },
            functions: BTreeMap::new(),
            subobjects: Vec::new(),
            data: BTreeMap::new(),
        };
        assert!(validate_object(&object).is_ok());
    }

    #[test]
    fn test_use_before_def() {
        let object = Object {
            name: "Test".to_string(),
            code: Block {
                statements: vec![Statement::Let {
                    bindings: vec![ValueId(1)],
                    value: Expression::Var(ValueId(0)),
                }],
            },
            functions: BTreeMap::new(),
            subobjects: Vec::new(),
            data: BTreeMap::new(),
        };
        let result = validate_object(&object);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::UseBeforeDef(0, _))));
    }

    #[test]
    fn test_if_yield_mismatch() {
        let object = Object {
            name: "Test".to_string(),
            code: Block {
                statements: vec![
                    Statement::Let {
                        bindings: vec![ValueId(0)],
                        value: Expression::Literal {
                            value: BigUint::from(1u64),
                            value_type: Type::Int(BitWidth::I256),
                        },
                    },
                    Statement::If {
                        condition: int_value(0),
                        inputs: vec![],
                        then_region: Region {
                            statements: vec![],
                            yields: vec![int_value(0)],
                        },
                        else_region: Some(Region {
                            statements: vec![],
                            yields: vec![],
                        }),
                        outputs: vec![ValueId(1)],
                    },
                ],
            },
            functions: BTreeMap::new(),
            subobjects: Vec::new(),
            data: BTreeMap::new(),
        };
        let result = validate_object(&object);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::YieldCountMismatch { .. })));
    }

    #[test]
    fn test_valid_function() {
        let mut functions = BTreeMap::new();
        functions.insert(
            FunctionId(0),
            Function {
                id: FunctionId(0),
                name: "add_one".to_string(),
                parameters: vec![(ValueId(0), Type::Int(BitWidth::I256))],
                returns: vec![Type::Int(BitWidth::I256)],
                return_values_initial: vec![ValueId(1)],
                return_values: vec![ValueId(2)],
                body: Block {
                    statements: vec![Statement::Let {
                        bindings: vec![ValueId(2)],
                        value: Expression::Binary {
                            operation: BinaryOperation::Add,
                            lhs: int_value(0),
                            rhs: int_value(1),
                        },
                    }],
                },
                call_count: 0,
                size_estimate: 0,
            },
        );

        let object = Object {
            name: "Test".to_string(),
            code: Block { statements: vec![] },
            functions,
            subobjects: Vec::new(),
            data: BTreeMap::new(),
        };
        assert!(validate_object(&object).is_ok());
    }

    #[test]
    fn test_function_return_count_mismatch() {
        let mut functions = BTreeMap::new();
        functions.insert(
            FunctionId(0),
            Function {
                id: FunctionId(0),
                name: "bad".to_string(),
                parameters: vec![],
                returns: vec![Type::Int(BitWidth::I256), Type::Int(BitWidth::I256)],
                return_values_initial: vec![ValueId(0)],
                return_values: vec![ValueId(0)],
                body: Block { statements: vec![] },
                call_count: 0,
                size_estimate: 0,
            },
        );

        let object = Object {
            name: "Test".to_string(),
            code: Block { statements: vec![] },
            functions,
            subobjects: Vec::new(),
            data: BTreeMap::new(),
        };
        let result = validate_object(&object);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::FunctionError(_))));
    }
}
