//! Type inference pass for narrowing integer widths.
//!
//! This module implements a dataflow-based type inference algorithm that
//! determines the minimum bit-width required for each SSA value. The algorithm:
//!
//! 1. **Forward pass**: Computes minimum width from literals and operation results
//! 2. **Backward pass**: Constrains width based on how values are USED
//! 3. Iterates until a fixed point is reached
//!
//! Key insight: If a value is only used in contexts needing N bits (e.g., mload offset
//! only needs 64 bits), we can constrain the value's computation to N bits.
//!
//! The result is that each value has the narrowest possible type that can
//! correctly represent all values it may hold at runtime AND satisfies all use sites.

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::{BinOp, BitWidth, Block, Expr, Function, Object, Region, Statement, Type, ValueId};

/// Type constraint representing the width bounds for a value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TypeConstraint {
    /// Minimum bit width required (from forward propagation).
    pub min_width: BitWidth,
    /// Maximum bit width needed (from backward propagation / use sites).
    /// If max_width < min_width, the value is truncated at use sites.
    pub max_width: BitWidth,
    /// Whether the value is known to be signed.
    pub is_signed: bool,
}

impl Default for TypeConstraint {
    fn default() -> Self {
        TypeConstraint {
            min_width: BitWidth::I1,   // Start with minimum
            max_width: BitWidth::I256, // Start with maximum
            is_signed: false,
        }
    }
}

impl TypeConstraint {
    /// Creates a constraint for a specific width.
    pub fn with_width(width: BitWidth) -> Self {
        TypeConstraint {
            min_width: width,
            max_width: BitWidth::I256,
            is_signed: false,
        }
    }

    /// Creates a signed constraint.
    pub fn signed(width: BitWidth) -> Self {
        TypeConstraint {
            min_width: width,
            max_width: BitWidth::I256,
            is_signed: true,
        }
    }

    /// Joins two constraints, taking the wider minimum.
    pub fn join(&self, other: &TypeConstraint) -> TypeConstraint {
        TypeConstraint {
            min_width: self.min_width.max(other.min_width),
            max_width: self.max_width.min(other.max_width), // Narrower max
            is_signed: self.is_signed || other.is_signed,
        }
    }

    /// Widens this constraint's minimum to at least the given width.
    pub fn widen_to(&mut self, width: BitWidth) -> bool {
        if width > self.min_width {
            self.min_width = width;
            true
        } else {
            false
        }
    }

    /// Narrows this constraint's maximum to at most the given width.
    pub fn narrow_max_to(&mut self, width: BitWidth) -> bool {
        if width < self.max_width {
            self.max_width = width;
            true
        } else {
            false
        }
    }

    /// Returns the effective width to use for this value.
    /// Takes the maximum of min_width (what we need to hold the value)
    /// and respects max_width (what use sites need).
    pub fn effective_width(&self) -> BitWidth {
        // The effective width is bounded by what the use sites need,
        // but must be at least what the value requires.
        self.min_width.max(self.max_width.min(self.min_width))
    }
}

/// Use context - how a value is used affects its max_width constraint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UseContext {
    /// Used as a memory offset (64-bit sufficient).
    MemoryOffset,
    /// Used as a memory value (256-bit required for EVM compatibility).
    MemoryValue,
    /// Used as a storage key or value (256-bit required).
    StorageAccess,
    /// Used in a comparison (keeps narrow type).
    Comparison,
    /// Used in arithmetic (may need full width depending on operation).
    Arithmetic,
    /// Used as function argument (depends on callee).
    FunctionArg,
    /// Returned from function (may escape, assume full width).
    FunctionReturn,
    /// Used in external call (256-bit for EVM ABI).
    ExternalCall,
    /// General/unknown use.
    General,
}

impl UseContext {
    /// Returns the maximum width needed for this use context.
    fn max_width_needed(&self) -> BitWidth {
        match self {
            UseContext::MemoryOffset => BitWidth::I64,
            UseContext::MemoryValue => BitWidth::I256,
            UseContext::StorageAccess => BitWidth::I256,
            UseContext::Comparison => BitWidth::I256, // Preserve, don't constrain
            UseContext::Arithmetic => BitWidth::I256, // Conservative
            UseContext::FunctionArg => BitWidth::I256, // Conservative for now
            UseContext::FunctionReturn => BitWidth::I256,
            UseContext::ExternalCall => BitWidth::I256,
            UseContext::General => BitWidth::I256,
        }
    }
}

/// Type inference context holding all constraints.
#[derive(Clone)]
pub struct TypeInference {
    /// Constraints for each value.
    constraints: BTreeMap<u32, TypeConstraint>,
    /// Use contexts for each value (for backward propagation).
    uses: BTreeMap<u32, BTreeSet<UseContext>>,
    /// Whether any constraint changed in the last iteration.
    changed: bool,
    /// Function parameter ValueIds, keyed by FunctionId.
    /// Used during the forward pass to propagate caller argument widths to callee parameters.
    function_params: BTreeMap<u32, Vec<(ValueId, Type)>>,
    /// Function return value IDs, keyed by FunctionId.
    /// Used during the forward pass to propagate return value widths to call sites.
    function_returns: BTreeMap<u32, Vec<ValueId>>,
}

impl TypeInference {
    /// Creates a new type inference context.
    pub fn new() -> Self {
        TypeInference {
            constraints: BTreeMap::new(),
            uses: BTreeMap::new(),
            changed: false,
            function_params: BTreeMap::new(),
            function_returns: BTreeMap::new(),
        }
    }

    /// Gets the constraint for a value, creating a default if none exists.
    pub fn get(&self, id: ValueId) -> TypeConstraint {
        self.constraints.get(&id.0).copied().unwrap_or_default()
    }

    /// Gets the effective width for a value (considering both min and max).
    pub fn effective_width(&self, id: ValueId) -> BitWidth {
        let constraint = self.get(id);
        // The effective width is the minimum required, but capped by use context
        constraint
            .min_width
            .max(constraint.max_width.min(constraint.min_width))
    }

    /// Sets the constraint for a value, returning true if changed.
    #[allow(dead_code)]
    fn set(&mut self, id: ValueId, constraint: TypeConstraint) -> bool {
        let existing = self.get(id);
        if constraint.min_width > existing.min_width
            || constraint.max_width < existing.max_width
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

    /// Records a use of a value in a specific context (for backward propagation).
    fn record_use(&mut self, id: ValueId, context: UseContext) {
        self.uses.entry(id.0).or_default().insert(context);
    }

    /// Narrows a value's max_width based on use context.
    fn narrow_from_use(&mut self, id: ValueId, max_width: BitWidth) -> bool {
        let mut constraint = self.get(id);
        if constraint.narrow_max_to(max_width) {
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

    /// Runs type inference on an object with both forward and backward passes.
    pub fn infer_object(&mut self, object: &Object) {
        // Pre-populate function_params so the forward pass can propagate
        // caller argument widths to callee parameters.
        for (func_id, function) in &object.functions {
            self.function_params
                .insert(func_id.0, function.params.clone());
        }

        // Pre-populate function_returns so the forward pass can propagate
        // return value widths to call sites interprocedurally.
        for (func_id, function) in &object.functions {
            if !function.return_values.is_empty() {
                self.function_returns
                    .insert(func_id.0, function.return_values.clone());
            }
        }

        // Phase 1: Forward propagation - determine minimum widths
        loop {
            self.changed = false;

            self.infer_block_forward(&object.code);

            for function in object.functions.values() {
                self.infer_function_forward(function);
            }

            for subobject in &object.subobjects {
                self.infer_object(subobject);
            }

            if !self.changed {
                break;
            }
        }

        // Phase 2: Backward propagation - collect uses and narrow max_width
        self.collect_uses_block(&object.code);
        for function in object.functions.values() {
            self.collect_uses_function(function);
        }

        // Phase 3: Apply backward constraints
        self.apply_backward_constraints();
    }

    /// Forward pass for a function.
    fn infer_function_forward(&mut self, function: &Function) {
        // Don't unconditionally widen params to I256.
        // Instead, let call sites determine parameter widths via Expr::Call propagation.
        // This enables interprocedural narrowing: if all callers pass values that fit in I32,
        // the parameter stays narrow, which produces a narrow LLVM function signature.
        //
        // Only widen to the declared type's width as a floor (for non-I256 types).
        for (param_id, param_ty) in &function.params {
            if let Type::Int(width) = param_ty {
                if *width < BitWidth::I256 {
                    self.widen(*param_id, *width);
                }
            }
        }

        self.infer_block_forward(&function.body);
    }

    /// Collect uses from a function for backward propagation.
    fn collect_uses_function(&mut self, function: &Function) {
        self.collect_uses_block(&function.body);

        // Return values escape to caller - mark as needing full width
        for ret_id in &function.return_values {
            self.record_use(*ret_id, UseContext::FunctionReturn);
        }
    }

    /// Apply backward constraints based on collected uses.
    fn apply_backward_constraints(&mut self) {
        for (id, uses) in &self.uses {
            // Find the minimum max_width across all uses
            let mut narrowest_max = BitWidth::I256;
            for use_ctx in uses {
                let needed = use_ctx.max_width_needed();
                if needed < narrowest_max {
                    narrowest_max = needed;
                }
            }

            // Only narrow if ALL uses allow it
            let can_narrow = uses.iter().all(|u| u.max_width_needed() <= narrowest_max);
            if can_narrow && narrowest_max < BitWidth::I256 {
                let mut constraint = self.constraints.get(id).copied().unwrap_or_default();
                constraint.narrow_max_to(narrowest_max);
                self.constraints.insert(*id, constraint);
            }
        }
    }

    /// Narrows function parameter types based on interprocedural analysis.
    ///
    /// Uses the forward-inferred min_width for each parameter, which reflects the
    /// maximum width across all call sites (propagated via Expr::Call during the
    /// forward pass). This is more precise than the backward use-context analysis
    /// because it narrows based on what callers actually PASS, not how the function
    /// internally USES the parameter.
    ///
    /// Safety: The function body zero-extends narrowed parameters back to word type,
    /// creating an implicit LLVM range proof. This is sound because:
    /// 1. All callers pass values fitting in the narrow width (by forward inference)
    /// 2. The zero-extension preserves the value
    /// 3. LLVM uses the range proof to eliminate downstream overflow checks
    pub fn narrow_function_params(&self, object: &mut Object) {
        for function in object.functions.values_mut() {
            for (param_id, param_ty) in &mut function.params {
                // Only narrow I256 integer parameters
                if !matches!(param_ty, Type::Int(BitWidth::I256)) {
                    continue;
                }

                let constraint = self.get(*param_id);

                // Skip signed values (truncation + zero-extension doesn't preserve sign)
                if constraint.is_signed {
                    continue;
                }

                // Use the forward-inferred min_width. This reflects the max width
                // across all call sites. Only narrow if it's strictly smaller than I256.
                let inferred = constraint.min_width;
                if inferred < BitWidth::I256 {
                    // Clamp to at least I32 for safety (XLEN is 32-bit on PolkaVM).
                    // Narrower types (I1, I8) can cause issues with LLVM calling conventions.
                    let clamped = inferred.max(BitWidth::I32);
                    *param_ty = Type::Int(clamped);
                }
            }
        }

        // Recurse into subobjects
        for subobject in &mut object.subobjects {
            self.narrow_function_params(subobject);
        }
    }

    /// Forward pass: infers minimum types for a block.
    fn infer_block_forward(&mut self, block: &Block) {
        for stmt in &block.statements {
            self.infer_statement_forward(stmt);
        }
    }

    /// Forward pass: infers minimum types for a region.
    fn infer_region_forward(&mut self, region: &Region) {
        for stmt in &region.statements {
            self.infer_statement_forward(stmt);
        }
    }

    /// Collects uses from a block for backward propagation.
    fn collect_uses_block(&mut self, block: &Block) {
        for stmt in &block.statements {
            self.collect_uses_statement(stmt);
        }
    }

    /// Collects uses from a region for backward propagation.
    fn collect_uses_region(&mut self, region: &Region) {
        for stmt in &region.statements {
            self.collect_uses_statement(stmt);
        }
    }

    /// Collects uses from a statement for backward propagation.
    fn collect_uses_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::MStore { offset, value, .. } | Statement::MStore8 { offset, value, .. } => {
                // Offset only needs 64 bits for memory addressing
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(offset.id, BitWidth::I64);
                // Value needs full 256 bits for EVM memory semantics
                self.record_use(value.id, UseContext::MemoryValue);
            }

            Statement::SStore { key, value, .. } | Statement::TStore { key, value } => {
                self.record_use(key.id, UseContext::StorageAccess);
                self.record_use(value.id, UseContext::StorageAccess);
            }

            Statement::If {
                condition,
                then_region,
                else_region,
                ..
            } => {
                // Condition only needs to be non-zero, can stay narrow
                self.record_use(condition.id, UseContext::Comparison);
                self.collect_uses_region(then_region);
                if let Some(else_region) = else_region {
                    self.collect_uses_region(else_region);
                }
            }

            Statement::Switch {
                scrutinee,
                cases,
                default,
                ..
            } => {
                self.record_use(scrutinee.id, UseContext::Comparison);
                for case in cases {
                    self.collect_uses_region(&case.body);
                }
                if let Some(default) = default {
                    self.collect_uses_region(default);
                }
            }

            Statement::For {
                init_values,
                condition_stmts,
                body,
                post,
                ..
            } => {
                for val in init_values {
                    self.record_use(val.id, UseContext::Arithmetic);
                }
                for stmt in condition_stmts {
                    self.collect_uses_statement(stmt);
                }
                self.collect_uses_region(body);
                self.collect_uses_region(post);
            }

            Statement::Revert { offset, length } | Statement::Return { offset, length } => {
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(offset.id, BitWidth::I64);
                self.record_use(length.id, UseContext::MemoryOffset);
                self.narrow_from_use(length.id, BitWidth::I64);
            }

            Statement::ExternalCall {
                gas,
                address,
                value,
                args_offset,
                args_length,
                ret_offset,
                ret_length,
                ..
            } => {
                self.record_use(gas.id, UseContext::ExternalCall);
                self.record_use(address.id, UseContext::ExternalCall);
                if let Some(value) = value {
                    self.record_use(value.id, UseContext::ExternalCall);
                }
                // Offsets and lengths are memory pointers
                self.record_use(args_offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(args_offset.id, BitWidth::I64);
                self.record_use(args_length.id, UseContext::MemoryOffset);
                self.narrow_from_use(args_length.id, BitWidth::I64);
                self.record_use(ret_offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(ret_offset.id, BitWidth::I64);
                self.record_use(ret_length.id, UseContext::MemoryOffset);
                self.narrow_from_use(ret_length.id, BitWidth::I64);
            }

            Statement::Log {
                offset,
                length,
                topics,
            } => {
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(offset.id, BitWidth::I64);
                self.record_use(length.id, UseContext::MemoryOffset);
                self.narrow_from_use(length.id, BitWidth::I64);
                for topic in topics {
                    self.record_use(topic.id, UseContext::MemoryValue); // Topics are 256-bit
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
                self.record_use(dest.id, UseContext::MemoryOffset);
                self.narrow_from_use(dest.id, BitWidth::I64);
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(offset.id, BitWidth::I64);
                self.record_use(length.id, UseContext::MemoryOffset);
                self.narrow_from_use(length.id, BitWidth::I64);
            }

            Statement::MCopy { dest, src, length } => {
                self.record_use(dest.id, UseContext::MemoryOffset);
                self.narrow_from_use(dest.id, BitWidth::I64);
                self.record_use(src.id, UseContext::MemoryOffset);
                self.narrow_from_use(src.id, BitWidth::I64);
                self.record_use(length.id, UseContext::MemoryOffset);
                self.narrow_from_use(length.id, BitWidth::I64);
            }

            Statement::Block(region) => {
                self.collect_uses_region(region);
            }

            Statement::Let { value, .. } => {
                self.collect_uses_expr(value);
            }

            Statement::Expr(expr) => {
                self.collect_uses_expr(expr);
            }

            _ => {}
        }
    }

    /// Collects uses from an expression.
    fn collect_uses_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Binary { lhs, rhs, op } => match op {
                BinOp::Lt | BinOp::Gt | BinOp::Slt | BinOp::Sgt | BinOp::Eq => {
                    self.record_use(lhs.id, UseContext::Comparison);
                    self.record_use(rhs.id, UseContext::Comparison);
                }
                _ => {
                    self.record_use(lhs.id, UseContext::Arithmetic);
                    self.record_use(rhs.id, UseContext::Arithmetic);
                }
            },
            Expr::Ternary { a, b, n, .. } => {
                self.record_use(a.id, UseContext::Arithmetic);
                self.record_use(b.id, UseContext::Arithmetic);
                self.record_use(n.id, UseContext::Arithmetic);
            }
            Expr::Unary { operand, .. } => {
                self.record_use(operand.id, UseContext::Arithmetic);
            }
            Expr::MLoad { offset, .. } => {
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(offset.id, BitWidth::I64);
            }
            Expr::SLoad { key, .. } | Expr::TLoad { key } => {
                self.record_use(key.id, UseContext::StorageAccess);
            }
            Expr::CallDataLoad { offset } => {
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(offset.id, BitWidth::I64);
            }
            Expr::Keccak256 { offset, length } => {
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(offset.id, BitWidth::I64);
                self.record_use(length.id, UseContext::MemoryOffset);
                self.narrow_from_use(length.id, BitWidth::I64);
            }
            Expr::Keccak256Pair { word0, word1 } => {
                self.record_use(word0.id, UseContext::FunctionArg);
                self.record_use(word1.id, UseContext::FunctionArg);
            }
            Expr::Keccak256Single { word0 } => {
                self.record_use(word0.id, UseContext::FunctionArg);
            }
            Expr::Call { args, .. } => {
                for arg in args {
                    self.record_use(arg.id, UseContext::FunctionArg);
                }
            }
            Expr::Balance { address }
            | Expr::ExtCodeSize { address }
            | Expr::ExtCodeHash { address } => {
                self.record_use(address.id, UseContext::ExternalCall);
            }
            Expr::BlockHash { number } | Expr::BlobHash { index: number } => {
                self.record_use(number.id, UseContext::MemoryOffset);
                self.narrow_from_use(number.id, BitWidth::I64);
            }
            _ => {}
        }
    }

    /// Forward pass: infers types for a statement.
    fn infer_statement_forward(&mut self, stmt: &Statement) {
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
                outputs,
                ..
            } => {
                // Condition only needs to be boolean-like
                self.widen(condition.id, BitWidth::I1);
                self.infer_region_forward(then_region);
                if let Some(else_region) = else_region {
                    self.infer_region_forward(else_region);
                }

                // Propagate region yield types to outputs.
                // Each output receives its value from the corresponding yield of
                // whichever branch executes.
                for region in std::iter::once(then_region).chain(else_region.as_ref()) {
                    for (yield_val, output) in region.yields.iter().zip(outputs.iter()) {
                        let yield_constraint = self.get(yield_val.id);
                        self.widen(*output, yield_constraint.min_width);
                    }
                }
            }

            Statement::Switch {
                scrutinee,
                cases,
                default,
                outputs,
                ..
            } => {
                // Switch value could be any size, but often fits in 64 bits
                self.widen(scrutinee.id, BitWidth::I64);
                for case in cases {
                    self.infer_region_forward(&case.body);
                    // Propagate case yields to outputs
                    for (yield_val, output) in case.body.yields.iter().zip(outputs.iter()) {
                        let yield_constraint = self.get(yield_val.id);
                        self.widen(*output, yield_constraint.min_width);
                    }
                }
                if let Some(default) = default {
                    self.infer_region_forward(default);
                    // Propagate default yields to outputs
                    for (yield_val, output) in default.yields.iter().zip(outputs.iter()) {
                        let yield_constraint = self.get(yield_val.id);
                        self.widen(*output, yield_constraint.min_width);
                    }
                }
            }

            Statement::For {
                loop_vars,
                condition,
                condition_stmts,
                body,
                post,
                post_input_vars,
                outputs,
                ..
            } => {
                // Loop variables participate in complex data flows across iterations.
                // Conservatively mark them as full width to avoid unsound narrowing.
                for loop_var in loop_vars {
                    self.widen(*loop_var, BitWidth::I256);
                }
                for post_var in post_input_vars {
                    self.widen(*post_var, BitWidth::I256);
                }
                // Loop outputs receive loop_var values when the loop exits,
                // so they must be at least as wide as loop_vars.
                for output in outputs {
                    self.widen(*output, BitWidth::I256);
                }

                // Infer widths for condition_stmts (Let bindings in the loop header).
                for stmt in condition_stmts {
                    self.infer_statement_forward(stmt);
                }
                let cond_width = self.infer_expr_width(condition);
                let _ = cond_width;

                self.infer_region_forward(body);
                self.infer_region_forward(post);
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
                self.infer_region_forward(region);
            }

            Statement::Expr(expr) => {
                let _ = self.infer_expr_width(expr);
            }

            // These don't define or use values
            Statement::Break { .. }
            | Statement::Continue { .. }
            | Statement::Leave { .. }
            | Statement::Stop
            | Statement::Invalid
            | Statement::PanicRevert { .. } => {}

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
                    BinOp::Add => {
                        // Addition can overflow by 1 bit
                        widen_by_one(lhs_width.max(rhs_width))
                    }
                    BinOp::Sub => {
                        // Subtraction wraps modular 2^256 when a < b,
                        // producing a full 256-bit result.
                        BitWidth::I256
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
                        // Shift right shrinks the value. In EVM, SHR(amount, value),
                        // lhs is shift amount, rhs is the value being shifted.
                        // Result width is bounded by the value's width.
                        rhs_width
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
                crate::ir::UnaryOp::Not => {
                    // NOT flips all 256 bits, producing a full-width result
                    let _ = operand;
                    BitWidth::I256
                }
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

            Expr::Call { function, args } => {
                // Propagate caller argument widths to callee parameters.
                // This enables interprocedural narrowing: if all callers pass small values,
                // the parameter type can be narrowed from I256.
                if let Some(callee_params) = self.function_params.get(&function.0).cloned() {
                    for (arg, (param_id, _param_ty)) in args.iter().zip(callee_params.iter()) {
                        let arg_width = self.get(arg.id).min_width;
                        self.widen(*param_id, arg_width);
                    }
                }

                // Propagate callee return value widths to call result.
                // If the callee's return values have known narrow widths (from the
                // forward pass over the callee body), use the widest return value's
                // width instead of I256. This narrows downstream operations at
                // call sites (e.g., sub(call_result, fmp) can become i64 arithmetic).
                if let Some(ret_ids) = self.function_returns.get(&function.0).cloned() {
                    let mut max_ret_width = BitWidth::I1;
                    for ret_id in &ret_ids {
                        let ret_width = self.get(*ret_id).min_width;
                        max_ret_width = max_ret_width.max(ret_width);
                    }
                    // Only narrow if we have meaningful return width info.
                    // Default (I1) means we haven't analyzed the callee yet;
                    // wait for the next iteration.
                    if max_ret_width > BitWidth::I1 {
                        max_ret_width
                    } else {
                        BitWidth::I256
                    }
                } else {
                    BitWidth::I256
                }
            }

            Expr::Truncate { to, .. } => *to,
            Expr::ZeroExtend { to, .. } => *to,
            Expr::SignExtendTo { to, .. } => *to,

            Expr::Keccak256 { offset, length } => {
                self.widen(offset.id, BitWidth::I64);
                self.widen(length.id, BitWidth::I64);
                BitWidth::I256
            }

            Expr::Keccak256Pair { .. } | Expr::Keccak256Single { .. } => BitWidth::I256,

            // DataOffset returns a contract code hash (256-bit), not an actual offset.
            Expr::DataOffset { .. } => BitWidth::I256,
            // DataSize returns header size (small value, fits in 64 bits).
            Expr::DataSize { .. } => BitWidth::I64,

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
pub fn widen_by_one(width: BitWidth) -> BitWidth {
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
pub fn double_width(width: BitWidth) -> BitWidth {
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
