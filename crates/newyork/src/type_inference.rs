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

use num::ToPrimitive;

use crate::ir::{
    for_each_statement, BinaryOperation, BitWidth, Block, Expression, Function, MemoryRegion,
    Object, Region, Statement, Type, UnaryOperation, ValueId,
};

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
            min_width: BitWidth::I1,
            max_width: BitWidth::I256,
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
            max_width: self.max_width.min(other.max_width),
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
    ///
    /// Uses the minimum of forward-propagated min_width and backward-propagated
    /// max_width, ensuring the value is at least as wide as what the definition
    /// requires but no wider than what use sites need.
    pub fn effective_width(&self) -> BitWidth {
        self.min_width.min(self.max_width)
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
            UseContext::MemoryOffset => BitWidth::I256,
            UseContext::MemoryValue => BitWidth::I256,
            UseContext::StorageAccess => BitWidth::I256,
            UseContext::Comparison => BitWidth::I256,
            UseContext::Arithmetic => BitWidth::I256,
            UseContext::FunctionArg => BitWidth::I256,
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
    /// Function return value IDs, keyed by FunctionId.
    /// Used during the forward pass to propagate return value widths to call sites.
    function_returns: BTreeMap<u32, Vec<ValueId>>,
    /// Per-value refined function-argument demand: the widest narrowed parameter width
    /// across all call sites where this value is passed as an argument.
    /// Set by `refine_demands_from_params` after parameter narrowing.
    /// When present, overrides UseContext::FunctionArg (I256) in `use_demand_width`.
    fn_arg_demand: BTreeMap<u32, BitWidth>,
    /// Known constant values (from Literal expressions), used for shift-amount
    /// analysis in forward inference. Only stores values that fit in u64.
    known_constants: BTreeMap<u32, u64>,
    /// Values that must keep full (I256) width because they feed an operand position
    /// where EVM tolerates out-of-range magnitudes that a narrowing truncation/​boundary
    /// trap would mis-handle: the SOURCE offset of `calldatacopy`/`codecopy` (zero-fill
    /// beyond source) and the shift amount / `byte` index / `signextend` byte position
    /// of `shl`/`shr`/`sar`/`byte`/`signextend` (out-of-range -> 0 / sign-fill /
    /// unchanged). Their `max_width` is forced back to I256 after backward constraints,
    /// overriding any narrowing demanded by other uses (e.g. a co-occurring memory
    /// offset). `effective = min(min_width, I256)`, so provably-small operands still
    /// narrow — only genuinely-wide ones stay full, keeping the OZ cost negligible.
    full_width_operands: BTreeSet<u32>,
    /// Type inference results for subobjects (each subobject has its own namespace).
    pub sub_inferences: Vec<TypeInference>,
}

impl TypeInference {
    /// Creates a new type inference context.
    pub fn new() -> Self {
        TypeInference {
            constraints: BTreeMap::new(),
            uses: BTreeMap::new(),
            changed: false,
            function_returns: BTreeMap::new(),
            fn_arg_demand: BTreeMap::new(),
            known_constants: BTreeMap::new(),
            full_width_operands: BTreeSet::new(),
            sub_inferences: Vec::new(),
        }
    }

    /// Gets the constraint for a value, creating a default if none exists.
    pub fn get(&self, id: ValueId) -> TypeConstraint {
        self.constraints.get(&id.0).copied().unwrap_or_default()
    }

    /// Gets the effective width for a value (considering both min and max).
    pub fn effective_width(&self, id: ValueId) -> BitWidth {
        let constraint = self.get(id);
        constraint
            .min_width
            .max(constraint.max_width.min(constraint.min_width))
    }

    /// Computes the widest width needed by any use site of this value.
    ///
    /// Unlike `max_width` on the constraint (which can be eagerly narrowed by
    /// `narrow_from_use` before all uses are collected), this method correctly
    /// examines ALL recorded use contexts and returns the WIDEST needed width.
    ///
    /// Every `UseContext` variant returns I256 from `max_width_needed`, so the
    /// only path that yields a result below I256 is `UseContext::FunctionArg`
    /// when `fn_arg_demand` carries a refined width for the value (populated
    /// by `refine_demands_from_params` after parameter narrowing). In
    /// particular, `MemoryOffset` deliberately stays at I256 — see the
    /// `MemoryOffset` arm of `max_width_needed` for why narrowing offsets
    /// here would bypass the use-site bounds check.
    pub fn use_demand_width(&self, id: ValueId) -> BitWidth {
        if let Some(uses) = self.uses.get(&id.0) {
            if uses.is_empty() {
                return BitWidth::I256;
            }
            let mut widest_needed = BitWidth::I1;
            for use_ctx in uses {
                let needed = match use_ctx {
                    UseContext::FunctionArg => self
                        .fn_arg_demand
                        .get(&id.0)
                        .copied()
                        .unwrap_or(BitWidth::I256),
                    _ => use_ctx.max_width_needed(),
                };
                if needed > widest_needed {
                    widest_needed = needed;
                }
            }
            widest_needed
        } else {
            BitWidth::I256
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
    ///
    /// Each object in the tree (deploy code, runtime code) has its own ValueId
    /// and FunctionId namespaces (both start at 0 per object). We process each
    /// object with a fresh context to avoid cross-object constraint pollution.
    ///
    /// After the backward constraints are applied, the final loop forces every operand in
    /// `full_width_operands` back to `I256`. These operands must keep full width regardless
    /// of any narrowing demanded by their other uses (e.g. the same value also feeding an
    /// `mload`): `calldatacopy`/`codecopy` source offsets so the saturating copy intrinsic
    /// can zero-fill beyond the source, and shift/`byte`/`signextend` amounts so EVM's
    /// out-of-range semantics are preserved.
    pub fn infer_object(&mut self, object: &Object) {
        for (function_id, function) in &object.functions {
            if !function.return_values.is_empty() {
                self.function_returns
                    .insert(function_id.0, function.return_values.clone());
            }
        }

        loop {
            self.changed = false;

            self.infer_block_forward(&object.code);

            for function in object.functions.values() {
                self.infer_function_forward(function);
            }

            if !self.changed {
                break;
            }
        }

        self.collect_uses_block(&object.code);
        for function in object.functions.values() {
            self.collect_uses_function(function);
        }

        self.propagate_use_demands(object);

        self.apply_backward_constraints();

        // Force the full-width operands back to I256 (see this method's doc comment).
        for id in &self.full_width_operands {
            let mut constraint = self.constraints.get(id).copied().unwrap_or_default();
            constraint.max_width = BitWidth::I256;
            self.constraints.insert(*id, constraint);
        }
    }

    /// Runs type inference on an object tree, including subobjects.
    ///
    /// Each subobject gets a fresh TypeInference context because different
    /// objects in the tree have overlapping ValueId/FunctionId namespaces
    /// (each object's translator allocates IDs starting from 0).
    pub fn infer_object_tree(&mut self, object: &Object) {
        self.infer_object(object);

        for subobject in &object.subobjects {
            let mut sub_inference = TypeInference::new();
            sub_inference.infer_object_tree(subobject);
            self.sub_inferences.push(sub_inference);
        }
    }

    /// Forward pass for a function.
    fn infer_function_forward(&mut self, function: &Function) {
        for (parameter_id, parameter_type) in &function.parameters {
            if let Type::Int(width) = parameter_type {
                self.widen(*parameter_id, *width);
            }
        }

        self.infer_block_forward(&function.body);
    }

    /// Collect uses from a function for backward propagation.
    fn collect_uses_function(&mut self, function: &Function) {
        self.collect_uses_block(&function.body);

        for ret_id in &function.return_values {
            self.record_use(*ret_id, UseContext::FunctionReturn);
        }
    }

    /// Apply backward constraints based on collected uses.
    fn apply_backward_constraints(&mut self) {
        for (id, uses) in &self.uses {
            let mut widest_needed = BitWidth::I1;
            for use_ctx in uses {
                let needed = match use_ctx {
                    UseContext::FunctionArg => self
                        .fn_arg_demand
                        .get(id)
                        .copied()
                        .unwrap_or(BitWidth::I256),
                    _ => use_ctx.max_width_needed(),
                };
                if needed > widest_needed {
                    widest_needed = needed;
                }
            }

            if widest_needed < BitWidth::I256 {
                let mut constraint = self.constraints.get(id).copied().unwrap_or_default();
                constraint.narrow_max_to(widest_needed);
                self.constraints.insert(*id, constraint);
            }
        }
    }

    /// Computes the widest backward demand excluding "transparent-for-parameters" uses.
    ///
    /// Used by `narrow_function_params` to determine if only Comparison/Arithmetic
    /// uses block parameter narrowing. Returns I256 if no uses recorded or
    /// any truly width-requiring use needs I256.
    ///
    /// Comparison uses are excluded because param narrowing inserts
    /// zero-extension at function entry, making comparison operations see the
    /// correct (zero-extended) value for in-range inputs.
    pub fn non_comparison_demand(&self, id: ValueId) -> BitWidth {
        if let Some(uses) = self.uses.get(&id.0) {
            if uses.is_empty() {
                return BitWidth::I256;
            }
            let mut widest = BitWidth::I1;
            for use_ctx in uses {
                if matches!(use_ctx, UseContext::Comparison) {
                    continue;
                }
                let needed = match use_ctx {
                    UseContext::FunctionArg => self
                        .fn_arg_demand
                        .get(&id.0)
                        .copied()
                        .unwrap_or(BitWidth::I256),
                    _ => use_ctx.max_width_needed(),
                };
                if needed > widest {
                    widest = needed;
                }
            }
            widest
        } else {
            BitWidth::I256
        }
    }

    /// Propagates use demands backward through transparent operations.
    ///
    /// For "transparent" operations (add, or, xor, and), the lower N bits of
    /// the result depend only on the lower N bits of the operands. This means
    /// if the result is only used in a context needing N bits (e.g., memory
    /// offset → I64), the operands also only need N bits.
    ///
    /// This enables parameter narrowing through add chains:
    /// `let pos := add(param, 32); mstore(pos, value)` → param only needs I64.
    fn propagate_use_demands(&mut self, object: &Object) {
        loop {
            let mut changed = false;
            changed |= self.propagate_demands_block(&object.code);
            for function in object.functions.values() {
                changed |= self.propagate_demands_block(&function.body);
            }
            if !changed {
                break;
            }
        }
    }

    fn propagate_demands_block(&mut self, block: &Block) -> bool {
        let mut changed = false;
        let mut snapshot: Vec<UseContext> = Vec::new();
        for_each_statement(&block.statements, &mut |statement| {
            if let Statement::Let { bindings, value } = statement {
                if bindings.len() == 1 {
                    let result_id = bindings[0];
                    snapshot.clear();
                    if let Some(uses) = self.uses.get(&result_id.0) {
                        snapshot.extend(uses.iter().copied());
                    }
                    if !snapshot.is_empty() {
                        changed |= self.propagate_demand_to_expression(value, &snapshot);
                    }
                }
            }
        });
        changed
    }

    /// Propagates result uses to operands of transparent expressions.
    fn propagate_demand_to_expression(
        &mut self,
        expression: &Expression,
        result_uses: &[UseContext],
    ) -> bool {
        match expression {
            Expression::Binary {
                lhs,
                rhs,
                operation:
                    BinaryOperation::Add
                    | BinaryOperation::Sub
                    | BinaryOperation::Mul
                    | BinaryOperation::Or
                    | BinaryOperation::Xor
                    | BinaryOperation::And,
            } => {
                let mut changed = false;
                for use_ctx in result_uses {
                    changed |= self.record_use_if_new(lhs.id, *use_ctx);
                    changed |= self.record_use_if_new(rhs.id, *use_ctx);
                }
                changed
            }
            Expression::Binary {
                rhs,
                operation: BinaryOperation::Shl,
                ..
            } => {
                let mut changed = false;
                for use_ctx in result_uses {
                    changed |= self.record_use_if_new(rhs.id, *use_ctx);
                }
                changed
            }
            Expression::Unary {
                operand,
                operation: UnaryOperation::Not,
            } => {
                let mut changed = false;
                for use_ctx in result_uses {
                    changed |= self.record_use_if_new(operand.id, *use_ctx);
                }
                changed
            }
            Expression::Var(id) => {
                let mut changed = false;
                for use_ctx in result_uses {
                    changed |= self.record_use_if_new(*id, *use_ctx);
                }
                changed
            }
            _ => false,
        }
    }

    /// Records a use context for a value, returning true if it was new.
    fn record_use_if_new(&mut self, id: ValueId, context: UseContext) -> bool {
        let entry = self.uses.entry(id.0).or_default();
        if entry.contains(&context) {
            false
        } else {
            entry.insert(context);
            true
        }
    }

    /// Narrows function parameter types based on backward demand analysis.
    ///
    /// Uses the backward-inferred max_width for each parameter, which reflects
    /// whether ALL use paths of the parameter are compatible with narrowing.
    /// The backward pass + demand propagation through transparent ops ensures
    /// that if ANY use path needs full width (storage values, external calls),
    /// max_width stays at I256 and prevents narrowing.
    ///
    /// For parameters where only `Comparison` uses block narrowing, a relaxed
    /// check is applied: if all non-comparison uses need ≤ I64 AND all callers
    /// provably pass values ≤ I64, narrowing is safe. The zero-extension in the
    /// function body preserves comparison semantics for values within the narrowed
    /// range (e.g., `gt(zext(param_i64), threshold)` is correct when param ≤ 2^64).
    ///
    /// Returns true if any parameter was narrowed.
    pub fn narrow_function_params(&self, object: &mut Object) -> bool {
        let mut changed = false;
        for function in object.functions.values_mut() {
            for (parameter_id, parameter_type) in &mut function.parameters {
                let constraint = self.get(*parameter_id);

                if !matches!(parameter_type, Type::Int(BitWidth::I256)) {
                    continue;
                }

                if constraint.is_signed {
                    continue;
                }

                let demand = constraint.max_width;
                if demand < BitWidth::I256 {
                    let clamped = demand.max(BitWidth::I32);
                    *parameter_type = Type::Int(clamped);
                    changed = true;
                }
            }
        }

        for (subobject, sub_inf) in object.subobjects.iter_mut().zip(self.sub_inferences.iter()) {
            changed |= sub_inf.narrow_function_params(subobject);
        }
        changed
    }

    /// Narrows function parameters based on call-site argument widths.
    ///
    /// For each function, examines ALL call sites and computes the forward
    /// min_width of each argument. If every caller passes an argument that
    /// provably fits in fewer than 256 bits, the parameter can be narrowed.
    ///
    /// This is the "forward" complement to the demand-based `narrow_function_params`:
    /// - `narrow_function_params`: narrows based on how values are USED inside the function
    /// - `narrow_function_params_from_callers`: narrows based on what callers PROVIDE
    ///
    /// Key use case: after guard_narrow inserts `and(value, 2^160-1)` for address
    /// validation, the call-site argument has min_width=I160. This pass detects
    /// that ALL callers provide I160 values and narrows the parameter to I160.
    pub fn narrow_function_params_from_callers(&self, object: &mut Object) -> bool {
        let mut argument_widths: BTreeMap<(u32, usize), BitWidth> = BTreeMap::new();
        let mut called_functions: BTreeSet<u32> = BTreeSet::new();

        self.collect_argument_widths_block(
            &object.code,
            &mut argument_widths,
            &mut called_functions,
        );
        for function in object.functions.values() {
            self.collect_argument_widths_block(
                &function.body,
                &mut argument_widths,
                &mut called_functions,
            );
        }

        let mut changed = false;
        for (&function_id, function) in &mut object.functions {
            if !called_functions.contains(&function_id.0) {
                continue;
            }
            for (i, (_, parameter_type)) in function.parameters.iter_mut().enumerate() {
                if !matches!(parameter_type, Type::Int(BitWidth::I256)) {
                    continue;
                }
                if let Some(&width) = argument_widths.get(&(function_id.0, i)) {
                    if width < BitWidth::I256 {
                        let clamped = width.max(BitWidth::I32);
                        *parameter_type = Type::Int(clamped);
                        changed = true;
                    }
                }
            }
        }

        for (subobject, sub_inf) in object.subobjects.iter_mut().zip(self.sub_inferences.iter()) {
            changed |= sub_inf.narrow_function_params_from_callers(subobject);
        }
        changed
    }

    /// Walks a block (and all nested regions) collecting argument min_widths
    /// at call sites. For each call to a function, records the widest argument
    /// seen across all callers — caller-driven narrowing uses the maximum.
    fn collect_argument_widths_block(
        &self,
        block: &Block,
        argument_widths: &mut BTreeMap<(u32, usize), BitWidth>,
        called_functions: &mut BTreeSet<u32>,
    ) {
        for_each_statement(&block.statements, &mut |statement| {
            let call = match statement {
                Statement::Let {
                    value:
                        Expression::Call {
                            function,
                            arguments,
                        },
                    ..
                }
                | Statement::Expression(Expression::Call {
                    function,
                    arguments,
                }) => Some((function, arguments)),
                _ => None,
            };
            if let Some((fid, arguments)) = call {
                called_functions.insert(fid.0);
                for (i, argument) in arguments.iter().enumerate() {
                    let arg_width = self.get(argument.id).min_width;
                    let entry = argument_widths.entry((fid.0, i)).or_insert(arg_width);
                    if arg_width > *entry {
                        *entry = arg_width;
                    }
                }
            }
        });
    }

    /// Narrows function return types based on forward min_width analysis.
    ///
    /// For each function with returns, examines the return value IDs' min_width
    /// from the forward pass. If the min_width is provably < I256, narrows
    /// the return type. This enables LLVM to use narrow return types (e.g., i64)
    /// instead of i256, reducing register pressure and spills.
    ///
    /// Safety: The forward pass's min_width represents the minimum bit-width
    /// required to represent the value based on the operations that produce it.
    /// Narrowing is safe because the function provably never produces values
    /// wider than min_width.
    ///
    /// A function returns via the fall-through `return_values` *and* via every early
    /// `leave`, which carries its own snapshot of the return variables. Narrowing must
    /// account for all of them — a `leave` can return a full-width value even when the
    /// fall-through value is narrow — so the loop below collects every return path's value
    /// id per position and narrows to the widest min_width across all of them.
    ///
    /// Returns true if any return type was narrowed.
    pub fn narrow_function_returns(&self, object: &mut Object) -> bool {
        let mut changed = false;
        for function in object.functions.values_mut() {
            let num_returns = function.returns.len();

            let mut return_path_ids: Vec<Vec<ValueId>> = vec![Vec::new(); num_returns];
            for (i, slot) in return_path_ids.iter_mut().enumerate() {
                if let Some(id) = function.return_values.get(i) {
                    slot.push(*id);
                }
            }
            crate::ir::for_each_statement(&function.body.statements, &mut |statement| {
                if let Statement::Leave { return_values } = statement {
                    for (i, value) in return_values.iter().enumerate() {
                        if let Some(slot) = return_path_ids.get_mut(i) {
                            slot.push(value.id);
                        }
                    }
                }
            });

            for (i, ret_ty) in function.returns.iter_mut().enumerate() {
                if !matches!(ret_ty, Type::Int(BitWidth::I256)) {
                    continue;
                }

                let ids = &return_path_ids[i];
                if ids.is_empty() {
                    continue;
                }

                // Any signed return path forces full width; the widest min_width across
                // all return paths bounds the narrowed type.
                if ids.iter().any(|id| self.get(*id).is_signed) {
                    continue;
                }

                let width = ids
                    .iter()
                    .map(|id| self.get(*id).min_width)
                    .max()
                    .unwrap_or(BitWidth::I256);
                if width < BitWidth::I256 {
                    let clamped = width.max(BitWidth::I32);
                    *ret_ty = Type::Int(clamped);
                    changed = true;
                }
            }
        }

        for (subobject, sub_inf) in object.subobjects.iter_mut().zip(self.sub_inferences.iter()) {
            changed |= sub_inf.narrow_function_returns(subobject);
        }
        changed
    }

    /// Narrows function return types based on backward demand analysis.
    ///
    /// For each function with I256 returns, examines ALL call sites to determine
    /// the widest use demand for each return value. If all callers only use the
    /// lower N bits (e.g., only as memory offsets needing I64), the return type
    /// can be narrowed to N bits.
    ///
    /// This is more aggressive than forward-only narrowing because it catches cases
    /// where a function internally computes I256 values (e.g., from SLOAD + arithmetic)
    /// but all callers only need narrow results.
    ///
    /// Safety: Narrowing truncates the return value. This is safe because:
    /// 1. All callers provably only use the lower N bits (backward demand analysis)
    /// 2. Comparisons are excluded (UseContext::Comparison demands I256)
    /// 3. External calls/storage are excluded (demand I256)
    ///
    /// Returns true if any return type was narrowed.
    pub fn narrow_function_returns_from_demand(&self, object: &mut Object) -> bool {
        let mut return_demands: BTreeMap<u32, Vec<BitWidth>> = BTreeMap::new();

        self.collect_return_demands_block(&object.code, &mut return_demands);
        for function in object.functions.values() {
            self.collect_return_demands_block(&function.body, &mut return_demands);
        }

        let mut changed = false;
        for function in object.functions.values_mut() {
            let demands = match return_demands.get(&function.id.0) {
                Some(d) => d,
                None => continue,
            };

            for (i, ret_ty) in function.returns.iter_mut().enumerate() {
                if !matches!(ret_ty, Type::Int(BitWidth::I256)) {
                    continue;
                }

                let demand = match demands.get(i) {
                    Some(d) => *d,
                    None => continue,
                };

                if demand < BitWidth::I256 {
                    let clamped = demand.max(BitWidth::I32);
                    *ret_ty = Type::Int(clamped);
                    changed = true;
                }
            }
        }

        for (subobject, sub_inf) in object.subobjects.iter_mut().zip(self.sub_inferences.iter()) {
            changed |= sub_inf.narrow_function_returns_from_demand(subobject);
        }
        changed
    }

    /// Walks a block (and all nested regions) collecting return demands from
    /// `Expression::Call` in `Let` statements. For each call binding, records the
    /// widest use-demand across all call sites — narrowing uses the maximum.
    fn collect_return_demands_block(
        &self,
        block: &Block,
        demands: &mut BTreeMap<u32, Vec<BitWidth>>,
    ) {
        for_each_statement(&block.statements, &mut |statement| {
            if let Statement::Let {
                bindings,
                value: Expression::Call { function, .. },
            } = statement
            {
                for (i, binding_id) in bindings.iter().enumerate() {
                    let demand = self.use_demand_width(*binding_id);
                    let entry = demands.entry(function.0).or_default();
                    while entry.len() <= i {
                        entry.push(BitWidth::I1);
                    }
                    entry[i] = entry[i].max(demand);
                }
            }
        });
    }

    /// Refines demand widths based on narrowed function parameter types.
    ///
    /// After `narrow_function_params` has narrowed function parameter types,
    /// this method re-examines all call sites and updates use demands for
    /// arguments. If a parameter was narrowed from I256 to I64, the argument
    /// only needs I64, not I256.
    ///
    /// This enables cascading demand narrowing: if a value is only passed to
    /// narrowed-parameter functions and used as memory offsets, it can be
    /// fully narrowed to I64 even though it was originally classified as
    /// FunctionArg (which defaults to I256).
    pub fn refine_demands_from_params(&mut self, object: &Object) {
        let parameter_widths: BTreeMap<u32, Vec<BitWidth>> = object
            .functions
            .iter()
            .map(|(function_id, function)| {
                let widths: Vec<BitWidth> = function
                    .parameters
                    .iter()
                    .map(|(_, value_type)| match value_type {
                        Type::Int(bit_width) => *bit_width,
                        _ => BitWidth::I256,
                    })
                    .collect();
                (function_id.0, widths)
            })
            .collect();

        self.refine_demands_in_block(&object.code, &parameter_widths);
        for function in object.functions.values() {
            self.refine_demands_in_block(&function.body, &parameter_widths);
        }

        self.propagate_use_demands(object);

        self.apply_backward_constraints();

        for (subobject, sub_inf) in object.subobjects.iter().zip(self.sub_inferences.iter_mut()) {
            sub_inf.refine_demands_from_params(subobject);
        }
    }

    /// Walks a block (and all nested regions) looking for Call expressions and
    /// updates argument demands based on the now-narrowed parameter widths.
    fn refine_demands_in_block(
        &mut self,
        block: &Block,
        parameter_widths: &BTreeMap<u32, Vec<BitWidth>>,
    ) {
        for_each_statement(&block.statements, &mut |statement| {
            statement.for_each_expression(&mut |expression| {
                self.refine_demands_in_expression(expression, parameter_widths);
            });
        });
    }

    /// Checks an expression for Call and updates argument demands.
    fn refine_demands_in_expression(
        &mut self,
        expression: &Expression,
        parameter_widths: &BTreeMap<u32, Vec<BitWidth>>,
    ) {
        if let Expression::Call {
            function,
            arguments,
        } = expression
        {
            if let Some(widths) = parameter_widths.get(&function.0) {
                for (argument, parameter_width) in arguments.iter().zip(widths.iter()) {
                    let entry = self
                        .fn_arg_demand
                        .entry(argument.id.0)
                        .or_insert(BitWidth::I1);
                    *entry = (*entry).max(*parameter_width);
                }
            } else {
                for argument in arguments {
                    let entry = self
                        .fn_arg_demand
                        .entry(argument.id.0)
                        .or_insert(BitWidth::I1);
                    *entry = BitWidth::I256;
                }
            }
        }
    }

    /// Forward pass: infers minimum types for a block.
    fn infer_block_forward(&mut self, block: &Block) {
        for statement in &block.statements {
            self.infer_statement_forward(statement);
        }
    }

    /// Forward pass: infers minimum types for a region.
    fn infer_region_forward(&mut self, region: &Region) {
        for statement in &region.statements {
            self.infer_statement_forward(statement);
        }
    }

    /// Collects uses from a block (recursing through nested regions and
    /// `For::condition_statements`) for backward propagation.
    fn collect_uses_block(&mut self, block: &Block) {
        for_each_statement(&block.statements, &mut |statement| {
            self.collect_uses_statement(statement);
        });
    }

    /// Collects uses from a single statement (no recursion — caller is
    /// responsible for walking nested regions, e.g. via `for_each_statement`).
    fn collect_uses_statement(&mut self, statement: &Statement) {
        match statement {
            Statement::MStore {
                offset,
                value,
                region,
            } => {
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(offset.id, BitWidth::I64);
                if *region == MemoryRegion::FreePointerSlot {
                    self.record_use(value.id, UseContext::MemoryOffset);
                    self.narrow_from_use(value.id, BitWidth::I64);
                } else {
                    self.record_use(value.id, UseContext::MemoryValue);
                }
            }
            Statement::MStore8 { offset, value, .. } => {
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(offset.id, BitWidth::I64);
                self.record_use(value.id, UseContext::MemoryValue);
            }

            Statement::SStore { key, value, .. } | Statement::TStore { key, value } => {
                self.record_use(key.id, UseContext::StorageAccess);
                self.record_use(value.id, UseContext::StorageAccess);
            }

            Statement::MappingSStore { key, slot, value } => {
                self.record_use(key.id, UseContext::StorageAccess);
                self.record_use(slot.id, UseContext::StorageAccess);
                self.record_use(value.id, UseContext::StorageAccess);
            }

            Statement::If { condition, .. } => {
                self.record_use(condition.id, UseContext::Comparison);
            }

            Statement::Switch { scrutinee, .. } => {
                self.record_use(scrutinee.id, UseContext::Comparison);
            }

            Statement::For { initial_values, .. } => {
                for value in initial_values {
                    self.record_use(value.id, UseContext::Arithmetic);
                }
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
                    self.record_use(topic.id, UseContext::MemoryValue);
                }
            }

            Statement::CallDataCopy {
                dest,
                offset,
                length,
            }
            | Statement::CodeCopy {
                dest,
                offset,
                length,
            } => {
                // Destination (heap pointer) and length (byte count) narrow to i64.
                self.record_use(dest.id, UseContext::MemoryOffset);
                self.narrow_from_use(dest.id, BitWidth::I64);
                self.record_use(length.id, UseContext::MemoryOffset);
                self.narrow_from_use(length.id, BitWidth::I64);
                // The SOURCE offset must stay full width: EVM zero-fills bytes beyond
                // the calldata/code source, so a >= 2^64 offset must reach the copy
                // intrinsic intact (it saturates and the host zero-fills). Narrowing it
                // would truncate the offset at its definition / the call boundary and
                // read the wrong source (or trap) instead of zero-filling.
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.full_width_operands.insert(offset.id.0);
            }

            Statement::ExtCodeCopy {
                dest,
                offset,
                length,
                ..
            }
            | Statement::ReturnDataCopy {
                dest,
                offset,
                length,
            } => {
                // `returndatacopy` reverts on an out-of-range source (and `extcodecopy`
                // is unsupported), so narrowing the source is safe: the use-site checked
                // truncation traps, matching EVM's revert.
                self.record_use(dest.id, UseContext::MemoryOffset);
                self.narrow_from_use(dest.id, BitWidth::I64);
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(offset.id, BitWidth::I64);
                self.record_use(length.id, UseContext::MemoryOffset);
                self.narrow_from_use(length.id, BitWidth::I64);
            }

            Statement::DataCopy {
                dest,
                offset,
                length,
            } => {
                self.record_use(dest.id, UseContext::MemoryOffset);
                self.narrow_from_use(dest.id, BitWidth::I64);
                self.record_use(offset.id, UseContext::MemoryValue);
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

            Statement::Block(_) => {}

            Statement::Let { value, .. } => {
                self.collect_uses_expression(value);
            }

            Statement::Expression(expression) => {
                self.collect_uses_expression(expression);
            }

            Statement::Create {
                value,
                offset,
                length,
                salt,
                ..
            } => {
                self.record_use(value.id, UseContext::ExternalCall);
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(offset.id, BitWidth::I64);
                self.record_use(length.id, UseContext::MemoryOffset);
                self.narrow_from_use(length.id, BitWidth::I64);
                if let Some(salt) = salt {
                    self.record_use(salt.id, UseContext::ExternalCall);
                }
            }

            Statement::SetImmutable { value, .. } => {
                self.record_use(value.id, UseContext::General);
            }

            Statement::SelfDestruct { address } => {
                self.record_use(address.id, UseContext::ExternalCall);
            }

            Statement::Leave { return_values } => {
                for value in return_values {
                    self.record_use(value.id, UseContext::FunctionReturn);
                }
            }

            Statement::Break { values } | Statement::Continue { values } => {
                for value in values {
                    self.record_use(value.id, UseContext::General);
                }
            }

            Statement::PanicRevert { .. }
            | Statement::ErrorStringRevert { .. }
            | Statement::CustomErrorRevert { .. }
            | Statement::Stop
            | Statement::Invalid => {}
        }
    }

    /// Collects uses from an expression.
    fn collect_uses_expression(&mut self, expression: &Expression) {
        match expression {
            Expression::Binary {
                lhs,
                rhs,
                operation,
            } => match operation {
                BinaryOperation::Lt
                | BinaryOperation::Gt
                | BinaryOperation::Slt
                | BinaryOperation::Sgt
                | BinaryOperation::Eq => {
                    self.record_use(lhs.id, UseContext::Comparison);
                    self.record_use(rhs.id, UseContext::Comparison);
                }
                BinaryOperation::Add
                | BinaryOperation::Sub
                | BinaryOperation::Mul
                | BinaryOperation::Or
                | BinaryOperation::Xor
                | BinaryOperation::And => {}
                BinaryOperation::Shl => {
                    self.record_use(lhs.id, UseContext::Arithmetic);
                    // The shift amount must keep full width: EVM defines a shift >= 256
                    // as 0, but if `lhs` is also narrowed (e.g. it doubles as a memory
                    // offset) a >= 256 amount is truncated/​trapped at the boundary,
                    // diverging from EVM.
                    self.full_width_operands.insert(lhs.id.0);
                }
                BinaryOperation::Shr
                | BinaryOperation::Sar
                | BinaryOperation::Byte
                | BinaryOperation::SignExtend => {
                    self.record_use(lhs.id, UseContext::Arithmetic);
                    self.record_use(rhs.id, UseContext::Arithmetic);
                    // The amount (shr/sar) / byte index / sign-extend byte position must
                    // keep full width: EVM handles out-of-range values specially (shift
                    // >= 256 -> 0/sign-fill; byte index >= 32 -> 0; signextend byte >= 31
                    // -> unchanged), so narrowing+truncating `lhs` diverges from EVM.
                    self.full_width_operands.insert(lhs.id.0);
                }
                _ => {
                    self.record_use(lhs.id, UseContext::Arithmetic);
                    self.record_use(rhs.id, UseContext::Arithmetic);
                }
            },
            Expression::Ternary { a, b, n, .. } => {
                self.record_use(a.id, UseContext::Arithmetic);
                self.record_use(b.id, UseContext::Arithmetic);
                self.record_use(n.id, UseContext::Arithmetic);
            }
            Expression::Unary { operand, operation } => match operation {
                UnaryOperation::Not => {}
                _ => {
                    self.record_use(operand.id, UseContext::Arithmetic);
                }
            },
            Expression::MLoad { offset, .. } => {
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(offset.id, BitWidth::I64);
            }
            Expression::SLoad { key, .. } | Expression::TLoad { key } => {
                self.record_use(key.id, UseContext::StorageAccess);
            }
            Expression::CallDataLoad { offset } => {
                self.record_use(offset.id, UseContext::Arithmetic);
            }
            Expression::Keccak256 { offset, length } => {
                self.record_use(offset.id, UseContext::MemoryOffset);
                self.narrow_from_use(offset.id, BitWidth::I64);
                self.record_use(length.id, UseContext::MemoryOffset);
                self.narrow_from_use(length.id, BitWidth::I64);
            }
            Expression::Keccak256Pair { word0, word1 } => {
                self.record_use(word0.id, UseContext::FunctionArg);
                self.record_use(word1.id, UseContext::FunctionArg);
            }
            Expression::Keccak256Single { word0 } => {
                self.record_use(word0.id, UseContext::FunctionArg);
            }
            Expression::MappingSLoad { key, slot } => {
                self.record_use(key.id, UseContext::FunctionArg);
                self.record_use(slot.id, UseContext::FunctionArg);
            }
            Expression::Call { arguments, .. } => {
                for argument in arguments {
                    self.record_use(argument.id, UseContext::FunctionArg);
                }
            }
            Expression::Balance { address }
            | Expression::ExtCodeSize { address }
            | Expression::ExtCodeHash { address } => {
                self.record_use(address.id, UseContext::ExternalCall);
            }
            Expression::BlockHash { number } => {
                self.record_use(number.id, UseContext::ExternalCall);
            }
            Expression::BlobHash { index: number } => {
                self.record_use(number.id, UseContext::MemoryOffset);
                self.narrow_from_use(number.id, BitWidth::I64);
            }
            _ => {}
        }
    }

    /// Forward pass: infers types for a statement.
    fn infer_statement_forward(&mut self, statement: &Statement) {
        match statement {
            Statement::Let { bindings, value } => {
                if let Expression::Literal { value: literal, .. } = value {
                    if let Some(v) = literal.to_u64() {
                        if bindings.len() == 1 {
                            self.known_constants.insert(bindings[0].0, v);
                        }
                    }
                }
                let expr_width = self.infer_expression_width(value);
                for binding in bindings {
                    self.widen(*binding, expr_width);
                }
            }

            Statement::MStore { offset, value, .. } => {
                self.widen(offset.id, BitWidth::I64);
                self.widen(value.id, BitWidth::I256);
            }

            Statement::MStore8 { offset, value, .. } => {
                self.widen(offset.id, BitWidth::I64);
                self.widen(value.id, BitWidth::I8);
            }

            Statement::SStore { key, value, .. } => {
                self.widen(key.id, BitWidth::I256);
                self.widen(value.id, BitWidth::I256);
            }

            Statement::TStore { key, value } => {
                self.widen(key.id, BitWidth::I256);
                self.widen(value.id, BitWidth::I256);
            }

            Statement::MappingSStore { key, slot, value } => {
                self.widen(key.id, BitWidth::I256);
                self.widen(slot.id, BitWidth::I256);
                self.widen(value.id, BitWidth::I256);
            }

            Statement::If {
                condition,
                then_region,
                else_region,
                outputs,
                ..
            } => {
                self.widen(condition.id, BitWidth::I1);
                self.infer_region_forward(then_region);
                if let Some(else_region) = else_region {
                    self.infer_region_forward(else_region);
                }

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
                self.widen(scrutinee.id, BitWidth::I64);
                for case in cases {
                    self.infer_region_forward(&case.body);
                    for (yield_val, output) in case.body.yields.iter().zip(outputs.iter()) {
                        let yield_constraint = self.get(yield_val.id);
                        self.widen(*output, yield_constraint.min_width);
                    }
                }
                if let Some(default) = default {
                    self.infer_region_forward(default);
                    for (yield_val, output) in default.yields.iter().zip(outputs.iter()) {
                        let yield_constraint = self.get(yield_val.id);
                        self.widen(*output, yield_constraint.min_width);
                    }
                }
            }

            Statement::For {
                loop_variables,
                condition,
                condition_statements,
                body,
                post,
                post_input_variables,
                outputs,
                ..
            } => {
                for loop_var in loop_variables {
                    self.widen(*loop_var, BitWidth::I256);
                }
                for post_var in post_input_variables {
                    self.widen(*post_var, BitWidth::I256);
                }
                for output in outputs {
                    self.widen(*output, BitWidth::I256);
                }

                for statement in condition_statements {
                    self.infer_statement_forward(statement);
                }
                let cond_width = self.infer_expression_width(condition);
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

            Statement::Expression(expression) => {
                let _ = self.infer_expression_width(expression);
            }

            Statement::Break { .. }
            | Statement::Continue { .. }
            | Statement::Leave { .. }
            | Statement::Stop
            | Statement::Invalid
            | Statement::PanicRevert { .. }
            | Statement::ErrorStringRevert { .. }
            | Statement::CustomErrorRevert { .. } => {}

            Statement::SetImmutable { value, .. } => {
                self.widen(value.id, BitWidth::I256);
            }
        }
    }

    /// Infers the minimum bit width for an expression result.
    fn infer_expression_width(&mut self, expression: &Expression) -> BitWidth {
        match expression {
            Expression::Literal { value, .. } => BitWidth::from_max_value(value),

            Expression::Var(id) => self.get(*id).min_width,

            Expression::Binary {
                operation,
                lhs,
                rhs,
            } => {
                let lhs_width = self.get(lhs.id).min_width;
                let rhs_width = self.get(rhs.id).min_width;

                let capped_operand_width = lhs_width.max(rhs_width).min(BitWidth::I64);

                match operation {
                    BinaryOperation::Add => {
                        self.widen(lhs.id, capped_operand_width);
                        self.widen(rhs.id, capped_operand_width);
                        widen_by_one(lhs_width.max(rhs_width))
                    }
                    BinaryOperation::Sub => BitWidth::I256,
                    BinaryOperation::Mul => {
                        self.widen(lhs.id, capped_operand_width);
                        self.widen(rhs.id, capped_operand_width);
                        double_width(lhs_width.max(rhs_width))
                    }
                    // Unsigned div/mod and signed mod are bounded by the
                    // dividend: the result magnitude never exceeds `lhs`. `smod`
                    // takes the sign of the dividend, so a non-negative (narrow)
                    // dividend yields a non-negative result <= dividend.
                    BinaryOperation::Div | BinaryOperation::Mod | BinaryOperation::SMod => {
                        lhs_width
                    }
                    // Signed division is NOT bounded by the dividend: a small
                    // non-negative dividend with a negative divisor yields a
                    // negative (full-width) quotient. A negative divisor has
                    // `rhs_width == I256`, so `max` correctly stays full-width;
                    // when both operands are narrow and non-negative the result
                    // is <= dividend < 2^max.
                    BinaryOperation::SDiv => lhs_width.max(rhs_width),
                    BinaryOperation::Exp => BitWidth::I256,

                    BinaryOperation::And => lhs_width.min(rhs_width),
                    BinaryOperation::Or | BinaryOperation::Xor => {
                        self.widen(lhs.id, capped_operand_width);
                        self.widen(rhs.id, capped_operand_width);
                        lhs_width.max(rhs_width)
                    }

                    BinaryOperation::Shl => BitWidth::I256,
                    // Logical right shift: the shifted-in high bits are zero, so
                    // a constant shift bounds the result to `256 - shift` bits.
                    BinaryOperation::Shr => {
                        if let Some(&shift) = self.known_constants.get(&lhs.id.0) {
                            if shift >= 256 {
                                BitWidth::I1
                            } else {
                                let remaining = 256u64.saturating_sub(shift);
                                BitWidth::from_bits(remaining.max(1) as u32)
                            }
                        } else {
                            rhs_width
                        }
                    }
                    // Arithmetic right shift sign-extends: shifting a *negative*
                    // value keeps the high bits set, so the result is NOT bounded
                    // by `256 - shift`. It is bounded by the operand's own width
                    // (`rhs`): a non-negative operand (rhs_width < 256) shifts
                    // down, a negative operand has rhs_width == I256.
                    BinaryOperation::Sar => rhs_width,

                    BinaryOperation::Lt
                    | BinaryOperation::Gt
                    | BinaryOperation::Slt
                    | BinaryOperation::Sgt
                    | BinaryOperation::Eq => {
                        if matches!(operation, BinaryOperation::Slt | BinaryOperation::Sgt) {
                            self.mark_signed(lhs.id);
                            self.mark_signed(rhs.id);
                        }
                        BitWidth::I1
                    }

                    BinaryOperation::Byte => BitWidth::I8,

                    BinaryOperation::SignExtend => BitWidth::I256,

                    BinaryOperation::AddMod | BinaryOperation::MulMod => BitWidth::I256,
                }
            }

            Expression::Ternary { operation, .. } => match operation {
                BinaryOperation::AddMod | BinaryOperation::MulMod => BitWidth::I256,
                _ => BitWidth::I256,
            },

            Expression::Unary { operation, operand } => match operation {
                crate::ir::UnaryOperation::IsZero => BitWidth::I1,
                crate::ir::UnaryOperation::Not => {
                    let _ = operand;
                    BitWidth::I256
                }
                crate::ir::UnaryOperation::Clz => BitWidth::I256,
            },

            Expression::CallDataLoad { offset } => {
                self.widen(offset.id, BitWidth::I256);
                BitWidth::I256
            }
            Expression::CallValue => BitWidth::I256,
            Expression::Caller | Expression::Origin | Expression::Address => BitWidth::I160,
            Expression::CallDataSize
            | Expression::CodeSize
            | Expression::ReturnDataSize
            | Expression::MSize => BitWidth::I64,
            Expression::GasPrice => BitWidth::I256,
            Expression::ExtCodeSize { address } => {
                self.widen(address.id, BitWidth::I160);
                BitWidth::I64
            }
            Expression::ExtCodeHash { address } => {
                self.widen(address.id, BitWidth::I160);
                BitWidth::I256
            }
            Expression::BlockHash { number } => {
                self.widen(number.id, BitWidth::I256);
                BitWidth::I256
            }
            Expression::Coinbase => BitWidth::I160,
            Expression::Timestamp | Expression::Number | Expression::GasLimit | Expression::Gas => {
                BitWidth::I64
            }
            Expression::Difficulty | Expression::ChainId | Expression::BaseFee => BitWidth::I256,
            Expression::SelfBalance | Expression::BlobBaseFee => BitWidth::I256,
            Expression::BlobHash { index } => {
                self.widen(index.id, BitWidth::I64);
                BitWidth::I256
            }
            Expression::Balance { address } => {
                self.widen(address.id, BitWidth::I160);
                BitWidth::I256
            }

            Expression::MLoad { offset, region } => {
                self.widen(offset.id, BitWidth::I64);
                if *region == MemoryRegion::FreePointerSlot {
                    BitWidth::I32
                } else {
                    BitWidth::I256
                }
            }
            Expression::SLoad { key, .. } => {
                self.widen(key.id, BitWidth::I256);
                BitWidth::I256
            }
            Expression::TLoad { key } => {
                self.widen(key.id, BitWidth::I256);
                BitWidth::I256
            }

            Expression::Call {
                function,
                arguments: _,
            } => {
                if let Some(ret_ids) = self.function_returns.get(&function.0).cloned() {
                    let mut max_ret_width = BitWidth::I1;
                    for ret_id in &ret_ids {
                        let ret_width = self.get(*ret_id).min_width;
                        max_ret_width = max_ret_width.max(ret_width);
                    }
                    if max_ret_width > BitWidth::I1 {
                        max_ret_width
                    } else {
                        BitWidth::I256
                    }
                } else {
                    BitWidth::I256
                }
            }

            Expression::Truncate { to, .. } => *to,
            Expression::ZeroExtend { to, .. } => *to,
            Expression::SignExtendTo { to, .. } => *to,

            Expression::Keccak256 { offset, length } => {
                self.widen(offset.id, BitWidth::I64);
                self.widen(length.id, BitWidth::I64);
                BitWidth::I256
            }

            Expression::Keccak256Pair { .. }
            | Expression::Keccak256Single { .. }
            | Expression::MappingSLoad { .. } => BitWidth::I256,

            Expression::DataOffset { .. } => BitWidth::I256,
            Expression::DataSize { .. } => BitWidth::I64,

            Expression::LoadImmutable { .. } => BitWidth::I256,

            Expression::LinkerSymbol { .. } => BitWidth::I160,
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
        BitWidth::I64 => BitWidth::I128,
        BitWidth::I128 => BitWidth::I160,
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
        BitWidth::I64 => BitWidth::I128,
        BitWidth::I128 => BitWidth::I256,
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

        let expression = Expression::Literal {
            value: BigUint::from(42u32),
            value_type: Type::default(),
        };
        let width = inference.infer_expression_width(&expression);
        assert_eq!(width, BitWidth::I8);

        let expression = Expression::Literal {
            value: BigUint::from(1u128) << 100,
            value_type: Type::default(),
        };
        let width = inference.infer_expression_width(&expression);
        assert_eq!(width, BitWidth::I128);

        let expression = Expression::Literal {
            value: BigUint::from(1u128) << 140,
            value_type: Type::default(),
        };
        let width = inference.infer_expression_width(&expression);
        assert_eq!(width, BitWidth::I160);
    }

    #[test]
    fn test_comparison_returns_boolean() {
        let mut inference = TypeInference::new();

        inference.widen(ValueId(0), BitWidth::I64);
        inference.widen(ValueId(1), BitWidth::I64);

        let expression = Expression::Binary {
            operation: BinaryOperation::Lt,
            lhs: crate::ir::Value::new(ValueId(0), Type::Int(BitWidth::I64)),
            rhs: crate::ir::Value::new(ValueId(1), Type::Int(BitWidth::I64)),
        };
        let width = inference.infer_expression_width(&expression);
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
