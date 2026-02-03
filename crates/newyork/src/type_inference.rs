//! Type inference pass for narrowing integer widths.
//!
//! This module implements a dataflow-based type inference algorithm that
//! determines the minimum bit-width required for each SSA value. The algorithm:
//!
//! 1. Starts with all values having an unknown (minimal) type
//! 2. Propagates type constraints forward and backward through the program
//! 3. Iterates until a fixed point is reached
//!
//! The result is that each value has the narrowest possible type that can
//! correctly represent all values it may hold at runtime.

use std::collections::BTreeMap;

use crate::ir::{BinOp, BitWidth, Block, Expr, Function, Object, Region, Statement, Type, ValueId};

/// Type constraint representing the minimum required width for a value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TypeConstraint {
    /// Minimum bit width required.
    pub min_width: BitWidth,
    /// Whether the value is known to be signed.
    pub is_signed: bool,
}

impl Default for TypeConstraint {
    fn default() -> Self {
        TypeConstraint {
            min_width: BitWidth::I1, // Start with minimum
            is_signed: false,
        }
    }
}

impl TypeConstraint {
    /// Creates a constraint for a specific width.
    pub fn with_width(width: BitWidth) -> Self {
        TypeConstraint {
            min_width: width,
            is_signed: false,
        }
    }

    /// Creates a signed constraint.
    pub fn signed(width: BitWidth) -> Self {
        TypeConstraint {
            min_width: width,
            is_signed: true,
        }
    }

    /// Joins two constraints, taking the wider one.
    pub fn join(&self, other: &TypeConstraint) -> TypeConstraint {
        TypeConstraint {
            min_width: self.min_width.max(other.min_width),
            is_signed: self.is_signed || other.is_signed,
        }
    }

    /// Widens this constraint to at least the given width.
    pub fn widen_to(&mut self, width: BitWidth) -> bool {
        if width > self.min_width {
            self.min_width = width;
            true
        } else {
            false
        }
    }
}

/// Type inference context holding all constraints.
pub struct TypeInference {
    /// Constraints for each value.
    constraints: BTreeMap<u32, TypeConstraint>,
    /// Whether any constraint changed in the last iteration.
    changed: bool,
}

impl TypeInference {
    /// Creates a new type inference context.
    pub fn new() -> Self {
        TypeInference {
            constraints: BTreeMap::new(),
            changed: false,
        }
    }

    /// Gets the constraint for a value, creating a default if none exists.
    pub fn get(&self, id: ValueId) -> TypeConstraint {
        self.constraints.get(&id.0).copied().unwrap_or_default()
    }

    /// Sets the constraint for a value, returning true if changed.
    fn set(&mut self, id: ValueId, constraint: TypeConstraint) -> bool {
        let existing = self.get(id);
        if constraint.min_width > existing.min_width
            || (constraint.is_signed && !existing.is_signed)
        {
            let joined = existing.join(&constraint);
            self.constraints.insert(id.0, joined);
            self.changed = true;
            true
        } else {
            false
        }
    }

    /// Widens a value's constraint to at least the given width.
    fn widen(&mut self, id: ValueId, width: BitWidth) -> bool {
        let mut constraint = self.get(id);
        if constraint.widen_to(width) {
            self.constraints.insert(id.0, constraint);
            self.changed = true;
            true
        } else {
            false
        }
    }

    /// Marks a value as signed.
    fn mark_signed(&mut self, id: ValueId) {
        let mut constraint = self.get(id);
        if !constraint.is_signed {
            constraint.is_signed = true;
            self.constraints.insert(id.0, constraint);
            self.changed = true;
        }
    }

    /// Runs type inference on an object.
    pub fn infer_object(&mut self, object: &Object) {
        // Run until fixed point
        loop {
            self.changed = false;

            // Infer types for main code block
            self.infer_block(&object.code);

            // Infer types for all functions
            for function in object.functions.values() {
                self.infer_function(function);
            }

            // Recursively handle subobjects
            for subobject in &object.subobjects {
                self.infer_object(subobject);
            }

            if !self.changed {
                break;
            }
        }
    }

    /// Infers types for a function.
    fn infer_function(&mut self, function: &Function) {
        // Parameters come from outside, assume full width for now
        for (param_id, param_ty) in &function.params {
            if let Type::Int(width) = param_ty {
                self.widen(*param_id, *width);
            } else {
                self.widen(*param_id, BitWidth::I256);
            }
        }

        self.infer_block(&function.body);
    }

    /// Infers types for a block.
    fn infer_block(&mut self, block: &Block) {
        for stmt in &block.statements {
            self.infer_statement(stmt);
        }
    }

    /// Infers types for a region.
    fn infer_region(&mut self, region: &Region) {
        for stmt in &region.statements {
            self.infer_statement(stmt);
        }
    }

    /// Infers types for a statement.
    fn infer_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Let { bindings, value } => {
                let expr_width = self.infer_expr_width(value);
                for binding in bindings {
                    self.widen(*binding, expr_width);
                }
            }

            Statement::MStore { offset, value, .. } => {
                // Offset is typically 32-bit or 64-bit
                self.widen(offset.id, BitWidth::I64);
                // Value stored to memory is full 256-bit
                self.widen(value.id, BitWidth::I256);
            }

            Statement::MStore8 { offset, value, .. } => {
                self.widen(offset.id, BitWidth::I64);
                // Only 8 bits are used from value
                self.widen(value.id, BitWidth::I8);
            }

            Statement::SStore { key, value, .. } => {
                // Storage keys and values are 256-bit
                self.widen(key.id, BitWidth::I256);
                self.widen(value.id, BitWidth::I256);
            }

            Statement::TStore { key, value } => {
                self.widen(key.id, BitWidth::I256);
                self.widen(value.id, BitWidth::I256);
            }

            Statement::If {
                condition,
                then_region,
                else_region,
                ..
            } => {
                // Condition only needs to be boolean-like
                self.widen(condition.id, BitWidth::I1);
                self.infer_region(then_region);
                if let Some(else_region) = else_region {
                    self.infer_region(else_region);
                }
            }

            Statement::Switch {
                scrutinee,
                cases,
                default,
                ..
            } => {
                // Switch value could be any size, but often fits in 64 bits
                self.widen(scrutinee.id, BitWidth::I64);
                for case in cases {
                    self.infer_region(&case.body);
                }
                if let Some(default) = default {
                    self.infer_region(default);
                }
            }

            Statement::For {
                init_values,
                loop_vars,
                condition,
                body,
                post,
                ..
            } => {
                // Propagate init value types to loop vars
                for (init_val, loop_var) in init_values.iter().zip(loop_vars.iter()) {
                    let init_constraint = self.get(init_val.id);
                    self.set(*loop_var, init_constraint);
                }

                // Condition only needs to be boolean-like
                let cond_width = self.infer_expr_width(condition);
                // But treat it as at least I1
                let _ = cond_width; // Condition result doesn't define new values

                self.infer_region(body);
                self.infer_region(post);
            }

            Statement::Revert { offset, length } | Statement::Return { offset, length } => {
                self.widen(offset.id, BitWidth::I64);
                self.widen(length.id, BitWidth::I64);
            }

            Statement::SelfDestruct { address } => {
                self.widen(address.id, BitWidth::I160);
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
                self.widen(gas.id, BitWidth::I64);
                self.widen(address.id, BitWidth::I160);
                if let Some(value) = value {
                    self.widen(value.id, BitWidth::I256);
                }
                self.widen(args_offset.id, BitWidth::I64);
                self.widen(args_length.id, BitWidth::I64);
                self.widen(ret_offset.id, BitWidth::I64);
                self.widen(ret_length.id, BitWidth::I64);
                // Result is boolean success/failure
                self.widen(*result, BitWidth::I1);
            }

            Statement::Create {
                value,
                offset,
                length,
                result,
                ..
            } => {
                self.widen(value.id, BitWidth::I256);
                self.widen(offset.id, BitWidth::I64);
                self.widen(length.id, BitWidth::I64);
                // Result is address or 0
                self.widen(*result, BitWidth::I160);
            }

            Statement::Log {
                offset,
                length,
                topics,
            } => {
                self.widen(offset.id, BitWidth::I64);
                self.widen(length.id, BitWidth::I64);
                for topic in topics {
                    self.widen(topic.id, BitWidth::I256);
                }
            }

            Statement::CodeCopy {
                dest,
                offset,
                length,
            }
            | Statement::ExtCodeCopy {
                dest,
                offset,
                length,
                ..
            }
            | Statement::ReturnDataCopy {
                dest,
                offset,
                length,
            }
            | Statement::DataCopy {
                dest,
                offset,
                length,
            }
            | Statement::CallDataCopy {
                dest,
                offset,
                length,
            } => {
                self.widen(dest.id, BitWidth::I64);
                self.widen(offset.id, BitWidth::I64);
                self.widen(length.id, BitWidth::I64);
            }

            Statement::MCopy { dest, src, length } => {
                self.widen(dest.id, BitWidth::I64);
                self.widen(src.id, BitWidth::I64);
                self.widen(length.id, BitWidth::I64);
            }

            Statement::Block(region) => {
                self.infer_region(region);
            }

            Statement::Expr(expr) => {
                let _ = self.infer_expr_width(expr);
            }

            // These don't define or use values
            Statement::Break
            | Statement::Continue
            | Statement::Leave { .. }
            | Statement::Stop
            | Statement::Invalid => {}

            Statement::SetImmutable { value, .. } => {
                // Immutable values are 256-bit
                self.widen(value.id, BitWidth::I256);
            }
        }
    }

    /// Infers the minimum bit width for an expression result.
    fn infer_expr_width(&mut self, expr: &Expr) -> BitWidth {
        match expr {
            Expr::Literal { value, .. } => {
                // Use the minimum width that can hold this literal
                BitWidth::from_max_value(value)
            }

            Expr::Var(id) => self.get(*id).min_width,

            Expr::Binary { op, lhs, rhs } => {
                let lhs_width = self.get(lhs.id).min_width;
                let rhs_width = self.get(rhs.id).min_width;

                match op {
                    // Arithmetic ops: result can be wider
                    BinOp::Add | BinOp::Sub => {
                        // Addition/subtraction can overflow by 1 bit
                        widen_by_one(lhs_width.max(rhs_width))
                    }
                    BinOp::Mul => {
                        // Multiplication doubles width
                        double_width(lhs_width.max(rhs_width))
                    }
                    BinOp::Div | BinOp::SDiv | BinOp::Mod | BinOp::SMod => {
                        // Division/modulo result fits in dividend width
                        lhs_width
                    }
                    BinOp::Exp => {
                        // Exponentiation can grow arbitrarily - assume full width
                        BitWidth::I256
                    }

                    // Bitwise ops: preserve width
                    BinOp::And => lhs_width.min(rhs_width), // AND shrinks to smaller
                    BinOp::Or | BinOp::Xor => lhs_width.max(rhs_width),

                    // Shifts
                    BinOp::Shl => {
                        // Shift left can grow the value
                        BitWidth::I256 // Conservative
                    }
                    BinOp::Shr | BinOp::Sar => {
                        // Shift right shrinks value
                        lhs_width
                    }

                    // Comparisons: result is boolean
                    BinOp::Lt | BinOp::Gt | BinOp::Slt | BinOp::Sgt | BinOp::Eq => {
                        // Mark signed ops
                        if matches!(op, BinOp::Slt | BinOp::Sgt) {
                            self.mark_signed(lhs.id);
                            self.mark_signed(rhs.id);
                        }
                        BitWidth::I1
                    }

                    // Byte extraction
                    BinOp::Byte => BitWidth::I8,

                    // Sign extension: can grow width
                    BinOp::SignExtend => BitWidth::I256,

                    // These are ternary ops, shouldn't be here
                    BinOp::AddMod | BinOp::MulMod => BitWidth::I256,
                }
            }

            Expr::Ternary { op, .. } => {
                match op {
                    // AddMod and MulMod results are bounded by the modulus
                    BinOp::AddMod | BinOp::MulMod => BitWidth::I256,
                    _ => BitWidth::I256,
                }
            }

            Expr::Unary { op, operand } => match op {
                crate::ir::UnaryOp::IsZero => BitWidth::I1,
                crate::ir::UnaryOp::Not => self.get(operand.id).min_width,
                crate::ir::UnaryOp::Clz => BitWidth::I256, // CLZ returns up to 256
            },

            // EVM builtins that return specific sizes
            Expr::CallDataLoad { offset } => {
                self.widen(offset.id, BitWidth::I64);
                BitWidth::I256
            }
            Expr::CallValue => BitWidth::I256,
            Expr::Caller | Expr::Origin | Expr::Address => BitWidth::I160,
            Expr::CallDataSize | Expr::CodeSize | Expr::ReturnDataSize | Expr::MSize => {
                BitWidth::I64
            }
            Expr::GasPrice => BitWidth::I256,
            Expr::ExtCodeSize { address } => {
                self.widen(address.id, BitWidth::I160);
                BitWidth::I64
            }
            Expr::ExtCodeHash { address } => {
                self.widen(address.id, BitWidth::I160);
                BitWidth::I256
            }
            Expr::BlockHash { number } => {
                self.widen(number.id, BitWidth::I64);
                BitWidth::I256
            }
            Expr::Coinbase => BitWidth::I160,
            Expr::Timestamp | Expr::Number | Expr::GasLimit | Expr::Gas => BitWidth::I64,
            Expr::Difficulty | Expr::ChainId | Expr::BaseFee => BitWidth::I256,
            Expr::SelfBalance | Expr::BlobBaseFee => BitWidth::I256,
            Expr::BlobHash { index } => {
                self.widen(index.id, BitWidth::I64);
                BitWidth::I256
            }
            Expr::Balance { address } => {
                self.widen(address.id, BitWidth::I160);
                BitWidth::I256
            }

            Expr::MLoad { offset, .. } => {
                self.widen(offset.id, BitWidth::I64);
                BitWidth::I256
            }
            Expr::SLoad { key, .. } => {
                self.widen(key.id, BitWidth::I256);
                BitWidth::I256
            }
            Expr::TLoad { key } => {
                self.widen(key.id, BitWidth::I256);
                BitWidth::I256
            }

            Expr::Call { args, .. } => {
                // Function calls could return anything
                for arg in args {
                    // Don't constrain args here, let the function definition do it
                    let _ = self.get(arg.id);
                }
                BitWidth::I256
            }

            Expr::Truncate { to, .. } => *to,
            Expr::ZeroExtend { to, .. } => *to,
            Expr::SignExtendTo { to, .. } => *to,

            Expr::Keccak256 { offset, length } => {
                self.widen(offset.id, BitWidth::I64);
                self.widen(length.id, BitWidth::I64);
                BitWidth::I256
            }

            Expr::DataOffset { .. } | Expr::DataSize { .. } => BitWidth::I64,

            Expr::LoadImmutable { .. } => BitWidth::I256, // Immutables are 256-bit

            Expr::LinkerSymbol { .. } => BitWidth::I160, // LinkerSymbol returns an address
        }
    }

    /// Returns the inferred type for a value.
    pub fn inferred_type(&self, id: ValueId) -> Type {
        let constraint = self.get(id);
        Type::Int(constraint.min_width)
    }

    /// Returns all constraints.
    pub fn constraints(&self) -> &BTreeMap<u32, TypeConstraint> {
        &self.constraints
    }
}

impl Default for TypeInference {
    fn default() -> Self {
        Self::new()
    }
}

/// Widens a bit width by one level (e.g., I8 -> I32).
fn widen_by_one(width: BitWidth) -> BitWidth {
    match width {
        BitWidth::I1 => BitWidth::I8,
        BitWidth::I8 => BitWidth::I32,
        BitWidth::I32 => BitWidth::I64,
        BitWidth::I64 => BitWidth::I160,
        BitWidth::I160 => BitWidth::I256,
        BitWidth::I256 => BitWidth::I256,
    }
}

/// Doubles a bit width (e.g., I32 -> I64).
fn double_width(width: BitWidth) -> BitWidth {
    match width {
        BitWidth::I1 => BitWidth::I8,
        BitWidth::I8 => BitWidth::I32,
        BitWidth::I32 => BitWidth::I64,
        BitWidth::I64 => BitWidth::I256,
        BitWidth::I160 => BitWidth::I256,
        BitWidth::I256 => BitWidth::I256,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num::BigUint;

    #[test]
    fn test_literal_width_inference() {
        let mut inference = TypeInference::new();

        // Small literal
        let expr = Expr::Literal {
            value: BigUint::from(42u32),
            ty: Type::default(),
        };
        let width = inference.infer_expr_width(&expr);
        assert_eq!(width, BitWidth::I8);

        // Large literal
        let expr = Expr::Literal {
            value: BigUint::from(1u128) << 100,
            ty: Type::default(),
        };
        let width = inference.infer_expr_width(&expr);
        assert_eq!(width, BitWidth::I160);
    }

    #[test]
    fn test_comparison_returns_boolean() {
        let mut inference = TypeInference::new();

        // Set up some values
        inference.widen(ValueId(0), BitWidth::I64);
        inference.widen(ValueId(1), BitWidth::I64);

        let expr = Expr::Binary {
            op: BinOp::Lt,
            lhs: crate::ir::Value::new(ValueId(0), Type::Int(BitWidth::I64)),
            rhs: crate::ir::Value::new(ValueId(1), Type::Int(BitWidth::I64)),
        };
        let width = inference.infer_expr_width(&expr);
        assert_eq!(width, BitWidth::I1);
    }

    #[test]
    fn test_constraint_join() {
        let c1 = TypeConstraint::with_width(BitWidth::I32);
        let c2 = TypeConstraint::signed(BitWidth::I64);
        let joined = c1.join(&c2);
        assert_eq!(joined.min_width, BitWidth::I64);
        assert!(joined.is_signed);
    }
}
