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

use crate::ir::{
    BinOp, BitWidth, Block, Expr, Function, MemoryRegion, Object, Region, Statement, Type, ValueId,
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
    /// Function return value IDs, keyed by FunctionId.
    /// Used during the forward pass to propagate return value widths to call sites.
    function_returns: BTreeMap<u32, Vec<ValueId>>,
    /// Per-value refined function-arg demand: the widest narrowed parameter width
    /// across all call sites where this value is passed as an argument.
    /// Set by `refine_demands_from_params` after parameter narrowing.
    /// When present, overrides UseContext::FunctionArg (I256) in `use_demand_width`.
    fn_arg_demand: BTreeMap<u32, BitWidth>,
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
        // The effective width is the minimum required, but capped by use context
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
    /// Returns I256 (conservative) if:
    /// - No uses are recorded (dead code or untracked pattern)
    /// - Any use requires full width (comparisons, storage, external calls, etc.)
    ///
    /// Only returns < I256 when ALL recorded uses allow narrowing (e.g., all are
    /// MemoryOffset which needs only I64).
    pub fn use_demand_width(&self, id: ValueId) -> BitWidth {
        if let Some(uses) = self.uses.get(&id.0) {
            if uses.is_empty() {
                return BitWidth::I256;
            }
            let mut widest_needed = BitWidth::I1;
            for use_ctx in uses {
                let needed = match use_ctx {
                    // If we have a refined function-arg demand for this value,
                    // use that instead of the blanket I256.
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
    ///
    /// Each object in the tree (deploy code, runtime code) has its own ValueId
    /// and FunctionId namespaces (both start at 0 per object). We process each
    /// object with a fresh context to avoid cross-object constraint pollution.
    pub fn infer_object(&mut self, object: &Object) {
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

            if !self.changed {
                break;
            }
        }

        // Phase 2: Backward propagation - collect uses and narrow max_width
        self.collect_uses_block(&object.code);
        for function in object.functions.values() {
            self.collect_uses_function(function);
        }

        // Phase 2.5: Propagate use demands through transparent operations.
        // For add/or/xor/and, the operands only need to be as wide as the result's
        // use sites require (modular arithmetic preserves lower bits). This enables
        // parameter narrowing through add chains: `let pos := add(param, 32);
        // mstore(pos, value)` → param only needs I64.
        self.propagate_use_demands(object);

        // Phase 3: Apply backward constraints
        self.apply_backward_constraints();
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
            // Store subobject results in our sub_inferences map for later use
            // by narrow_function_params and codegen.
            self.sub_inferences.push(sub_inference);
        }
    }

    /// Forward pass for a function.
    fn infer_function_forward(&mut self, function: &Function) {
        // Widen parameters to their declared type width in the forward pass.
        // This ensures min_width (used by `inferred_width` in codegen) reflects
        // the true runtime range — params can hold any value their type allows.
        // Without this, `narrow_offset_for_pointer` may unsoundly truncate
        // param-derived values that the forward analysis under-approximates.
        //
        // Note: param NARROWING is driven by the backward max_width (Phase 2.5),
        // not the forward min_width. So this does not prevent narrowing.
        for (param_id, param_ty) in &function.params {
            if let Type::Int(width) = param_ty {
                self.widen(*param_id, *width);
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
            // Find the widest width needed across all uses.
            // For FunctionArg, use fn_arg_demand if available (from
            // refine_demands_from_params) instead of the conservative I256.
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
        for stmt in &block.statements {
            changed |= self.propagate_demands_statement(stmt);
        }
        changed
    }

    fn propagate_demands_region(&mut self, region: &Region) -> bool {
        let mut changed = false;
        for stmt in &region.statements {
            changed |= self.propagate_demands_statement(stmt);
        }
        changed
    }

    fn propagate_demands_statement(&mut self, stmt: &Statement) -> bool {
        let mut changed = false;
        match stmt {
            Statement::Let { bindings, value } if bindings.len() == 1 => {
                let result_id = bindings[0];
                if let Some(result_uses) = self.uses.get(&result_id.0).cloned() {
                    changed |= self.propagate_demand_to_expr(value, &result_uses);
                }
            }
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                changed |= self.propagate_demands_region(then_region);
                if let Some(else_region) = else_region {
                    changed |= self.propagate_demands_region(else_region);
                }
            }
            Statement::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= self.propagate_demands_region(&case.body);
                }
                if let Some(default) = default {
                    changed |= self.propagate_demands_region(default);
                }
            }
            Statement::For {
                condition_stmts,
                body,
                post,
                ..
            } => {
                for stmt in condition_stmts {
                    changed |= self.propagate_demands_statement(stmt);
                }
                changed |= self.propagate_demands_region(body);
                changed |= self.propagate_demands_region(post);
            }
            Statement::Block(region) => {
                changed |= self.propagate_demands_region(region);
            }
            _ => {}
        }
        changed
    }

    /// Propagates result uses to operands of transparent expressions.
    fn propagate_demand_to_expr(
        &mut self,
        expr: &Expr,
        result_uses: &BTreeSet<UseContext>,
    ) -> bool {
        match expr {
            Expr::Binary {
                lhs,
                rhs,
                op: BinOp::Add | BinOp::Or | BinOp::Xor | BinOp::And,
            } => {
                let mut changed = false;
                for use_ctx in result_uses {
                    changed |= self.record_use_if_new(lhs.id, *use_ctx);
                    changed |= self.record_use_if_new(rhs.id, *use_ctx);
                }
                changed
            }
            Expr::Var(id) => {
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
    /// that if ANY use path needs full width (comparisons, iszero, storage
    /// values), max_width stays at I256 and prevents narrowing.
    ///
    /// Safety: The function body zero-extends narrowed parameters back to word type,
    /// creating an implicit LLVM range proof. This is sound because:
    /// 1. All use paths only observe the lower N bits (proven by backward analysis)
    /// 2. Transparent ops (add/or/xor/and) preserve lower-bit equivalence
    /// 3. LLVM uses the range proof to eliminate downstream overflow checks
    ///
    /// Returns true if any parameter was narrowed.
    pub fn narrow_function_params(&self, object: &mut Object) -> bool {
        let mut changed = false;
        for function in object.functions.values_mut() {
            for (param_id, param_ty) in &mut function.params {
                let constraint = self.get(*param_id);

                // Only narrow I256 integer parameters
                if !matches!(param_ty, Type::Int(BitWidth::I256)) {
                    continue;
                }

                // Skip signed values (truncation + zero-extension doesn't preserve sign)
                if constraint.is_signed {
                    continue;
                }

                // Use backward max_width: only narrow if ALL use paths allow it.
                // If any use path needs full width (comparisons, storage, etc.),
                // max_width stays at I256 and we don't narrow.
                let demand = constraint.max_width;
                if demand >= BitWidth::I256 {
                    continue;
                }

                // Clamp to at least I32 for safety (XLEN is 32-bit on PolkaVM).
                // Narrower types (I1, I8) can cause issues with LLVM calling conventions.
                let clamped = demand.max(BitWidth::I32);
                *param_ty = Type::Int(clamped);
                changed = true;
            }
        }

        // Recurse into subobjects using their scoped type inference contexts
        for (subobject, sub_inf) in object.subobjects.iter_mut().zip(self.sub_inferences.iter()) {
            changed |= sub_inf.narrow_function_params(subobject);
        }
        changed
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
        // Build a map from FunctionId -> parameter widths (after narrowing)
        let param_widths: BTreeMap<u32, Vec<BitWidth>> = object
            .functions
            .iter()
            .map(|(func_id, function)| {
                let widths: Vec<BitWidth> = function
                    .params
                    .iter()
                    .map(|(_, ty)| match ty {
                        Type::Int(bw) => *bw,
                        _ => BitWidth::I256,
                    })
                    .collect();
                (func_id.0, widths)
            })
            .collect();

        // Walk all code and find Expr::Call sites, updating argument demands
        self.refine_demands_in_block(&object.code, &param_widths);
        for function in object.functions.values() {
            self.refine_demands_in_block(&function.body, &param_widths);
        }

        // Re-run demand propagation through transparent ops with updated demands
        self.propagate_use_demands(object);

        // Re-apply backward constraints with updated use info
        self.apply_backward_constraints();

        // Recurse into subobjects
        for (subobject, sub_inf) in object.subobjects.iter().zip(self.sub_inferences.iter_mut()) {
            sub_inf.refine_demands_from_params(subobject);
        }
    }

    /// Walks a block looking for Call expressions and updates argument demands.
    fn refine_demands_in_block(
        &mut self,
        block: &Block,
        param_widths: &BTreeMap<u32, Vec<BitWidth>>,
    ) {
        for stmt in &block.statements {
            self.refine_demands_in_statement(stmt, param_widths);
        }
    }

    /// Walks a region looking for Call expressions and updates argument demands.
    fn refine_demands_in_region(
        &mut self,
        region: &Region,
        param_widths: &BTreeMap<u32, Vec<BitWidth>>,
    ) {
        for stmt in &region.statements {
            self.refine_demands_in_statement(stmt, param_widths);
        }
    }

    /// Walks a statement looking for Call expressions and updates argument demands.
    fn refine_demands_in_statement(
        &mut self,
        stmt: &Statement,
        param_widths: &BTreeMap<u32, Vec<BitWidth>>,
    ) {
        match stmt {
            Statement::Let { value, .. } => {
                self.refine_demands_in_expr(value, param_widths);
            }
            Statement::Expr(expr) => {
                self.refine_demands_in_expr(expr, param_widths);
            }
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                self.refine_demands_in_region(then_region, param_widths);
                if let Some(r) = else_region {
                    self.refine_demands_in_region(r, param_widths);
                }
            }
            Statement::Switch { cases, default, .. } => {
                for c in cases {
                    self.refine_demands_in_region(&c.body, param_widths);
                }
                if let Some(d) = default {
                    self.refine_demands_in_region(d, param_widths);
                }
            }
            Statement::For {
                condition_stmts,
                body,
                post,
                ..
            } => {
                for s in condition_stmts {
                    self.refine_demands_in_statement(s, param_widths);
                }
                self.refine_demands_in_region(body, param_widths);
                self.refine_demands_in_region(post, param_widths);
            }
            Statement::Block(region) => {
                self.refine_demands_in_region(region, param_widths);
            }
            _ => {}
        }
    }

    /// Checks an expression for Call and updates argument demands.
    fn refine_demands_in_expr(&mut self, expr: &Expr, param_widths: &BTreeMap<u32, Vec<BitWidth>>) {
        if let Expr::Call { function, args } = expr {
            if let Some(widths) = param_widths.get(&function.0) {
                // Check if ALL parameters are narrowed (< I256)
                let all_narrow = args
                    .iter()
                    .zip(widths.iter())
                    .all(|(_, w)| *w < BitWidth::I256);
                if !all_narrow {
                    // If any parameter is still I256, we can't narrow the FunctionArg demand
                    // for this specific call. But we still refine individual args below.
                }

                for (arg, param_width) in args.iter().zip(widths.iter()) {
                    // Track the widest narrowed parameter demand for this argument value.
                    // A value may be passed as arg to multiple functions - we take the widest.
                    let entry = self.fn_arg_demand.entry(arg.id.0).or_insert(BitWidth::I1);
                    *entry = (*entry).max(*param_width);
                }
            } else {
                // Unknown function (possibly a call to a function not in our object)
                // Mark args as needing full width
                for arg in args {
                    let entry = self.fn_arg_demand.entry(arg.id.0).or_insert(BitWidth::I1);
                    *entry = BitWidth::I256;
                }
            }
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

            // DataCopy is special: the offset field carries the contract code hash
            // (256-bit), not a memory offset. Only dest is a memory pointer.
            Statement::DataCopy {
                dest,
                offset,
                length,
            } => {
                self.record_use(dest.id, UseContext::MemoryOffset);
                self.narrow_from_use(dest.id, BitWidth::I64);
                // Offset is the contract code hash — needs full 256-bit width
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

            Statement::Block(region) => {
                self.collect_uses_region(region);
            }

            Statement::Let { value, .. } => {
                self.collect_uses_expr(value);
            }

            Statement::Expr(expr) => {
                self.collect_uses_expr(expr);
            }

            // Create operations: value is ETH sent, offset/length are memory pointers
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

            // SetImmutable: value needs full width (stored as contract data)
            Statement::SetImmutable { value, .. } => {
                self.record_use(value.id, UseContext::General);
            }

            // SelfDestruct: address needs full width
            Statement::SelfDestruct { address } => {
                self.record_use(address.id, UseContext::ExternalCall);
            }

            // Leave: return values escape to caller, need full width
            Statement::Leave { return_values } => {
                for val in return_values {
                    self.record_use(val.id, UseContext::FunctionReturn);
                }
            }

            // Break/Continue: carry loop variables, need full width
            Statement::Break { values } | Statement::Continue { values } => {
                for val in values {
                    self.record_use(val.id, UseContext::General);
                }
            }

            // PanicRevert: no variable values (code is u8 constant)
            // Stop/Invalid: no values
            Statement::PanicRevert { .. }
            | Statement::ErrorStringRevert { .. }
            | Statement::CustomErrorRevert { .. }
            | Statement::Stop
            | Statement::Invalid => {}
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
                // Transparent ops: don't record Arithmetic for operands.
                // Their demand will be propagated from the Let binding result's
                // uses in propagate_use_demands. This enables parameter narrowing
                // through add/or chains that flow to memory offsets.
                // Property: for these ops, trunc(op(a,b), N) == op(trunc(a,N), trunc(b,N))
                // when only the lower N bits of the result are observed.
                BinOp::Add | BinOp::Or | BinOp::Xor | BinOp::And => {}
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
                // Do NOT narrow the offset: calldataload's offset is a full 256-bit
                // EVM value. The clip_to_xlen overflow check must see all 256 bits
                // to correctly clamp out-of-range offsets to 0xFFFFFFFF. Truncating
                // to i64 first would lose upper bits and produce wrong clamp results
                // (e.g., 0xa3<<248 truncated to i64 becomes 0, but should clamp to
                // 0xFFFFFFFF because the original value > 0xFFFFFFFF).
                self.record_use(offset.id, UseContext::Arithmetic);
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
            | Statement::PanicRevert { .. }
            | Statement::ErrorStringRevert { .. }
            | Statement::CustomErrorRevert { .. } => {}

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

                // Operand widening: widen both operands to match each other.
                // This propagates meaningful widths from literals/known-width values
                // to function params. Capped at I64 to prevent mload-I256 pollution
                // (mload returns I256 but most values fit in I64 for memory ops).
                let capped_operand_width = lhs_width.max(rhs_width).min(BitWidth::I64);

                match op {
                    // Arithmetic ops: result can be wider.
                    BinOp::Add => {
                        self.widen(lhs.id, capped_operand_width);
                        self.widen(rhs.id, capped_operand_width);
                        // Addition can overflow by 1 bit
                        widen_by_one(lhs_width.max(rhs_width))
                    }
                    BinOp::Sub => {
                        // Subtraction wraps modular 2^256 when a < b,
                        // producing a full 256-bit result.
                        BitWidth::I256
                    }
                    BinOp::Mul => {
                        self.widen(lhs.id, capped_operand_width);
                        self.widen(rhs.id, capped_operand_width);
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
                    BinOp::Or | BinOp::Xor => {
                        self.widen(lhs.id, capped_operand_width);
                        self.widen(rhs.id, capped_operand_width);
                        lhs_width.max(rhs_width)
                    }

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

                    // Comparisons: result is boolean.
                    // Don't widen operands - comparisons can compare values of
                    // different widths (the codegen zero-extends as needed).
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
                // The offset must stay at full 256-bit width so that clip_to_xlen
                // can correctly clamp out-of-range offsets to 0xFFFFFFFF.
                self.widen(offset.id, BitWidth::I256);
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

            Expr::MLoad { offset, region } => {
                self.widen(offset.id, BitWidth::I64);
                // The free memory pointer (mload(64)) is bounded by the heap size.
                // On PolkaVM with a 128KB heap, the FMP value fits in I32 (actually I17).
                // Returning I32 here enables interprocedural narrowing: functions that
                // receive FMP-derived values get narrower parameters, eliminating
                // overflow checks in callees like abi_encode_string_memory_ptr_runtime.
                if *region == MemoryRegion::FreePointerSlot {
                    BitWidth::I32
                } else {
                    BitWidth::I256
                }
            }
            Expr::SLoad { key, .. } => {
                self.widen(key.id, BitWidth::I256);
                BitWidth::I256
            }
            Expr::TLoad { key } => {
                self.widen(key.id, BitWidth::I256);
                BitWidth::I256
            }

            Expr::Call { function, args: _ } => {
                // Parameter widths are determined by the function body's own forward
                // pass (how params are used internally), NOT by caller arg widths.
                // Propagating caller arg widths is unsound: the same SSA value may be
                // used in both narrow (e.g., memory offset) and wide (e.g., call value)
                // contexts, and the wider use pollutes the param constraint.

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
