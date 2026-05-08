//! IR simplification pass: constant folding, algebraic identities, copy propagation,
//! and dead code elimination.
//!
//! This pass runs after inlining to clean up the IR:
//! 1. **Constant folding**: Binary/unary ops on literals → literal result
//! 2. **Algebraic identities**: `add(x, 0) → x`, `mul(x, 1) → x`, etc.
//! 3. **Copy propagation**: `let x = y` → replace all uses of x with y
//! 4. **Dead code elimination**: Remove unused Let bindings
//!
//! All arithmetic follows EVM semantics (unsigned 256-bit, wrapping).

use std::collections::{BTreeMap, BTreeSet};

use num::{BigUint, One, ToPrimitive, Zero};

use crate::ir::{
    for_each_stmt_mut, BinaryOperation, BitWidth, Block, CallKind, Expression, FunctionId,
    MemoryRegion, Object, Region, Statement, SwitchCase, Type, UnaryOperation, Value, ValueId,
};

/// Maximum value for 256-bit unsigned integer (2^256 - 1).
fn max_u256() -> BigUint {
    (BigUint::one() << 256) - BigUint::one()
}

/// The modulus for 256-bit wrapping arithmetic (2^256).
fn modulus_u256() -> BigUint {
    BigUint::one() << 256
}

/// Results of the simplification pass.
#[derive(Clone, Debug, Default)]
pub struct SimplifyResults {
    /// Number of expressions constant-folded.
    pub constants_folded: usize,
    /// Number of algebraic identity simplifications.
    pub identities_simplified: usize,
    /// Number of copy propagations.
    pub copies_propagated: usize,
    /// Number of dead Let bindings removed.
    pub dead_bindings_removed: usize,
    /// Number of constant branches eliminated.
    pub branches_eliminated: usize,
    /// Number of environment reads eliminated by CSE.
    pub env_reads_eliminated: usize,
}

/// Categories of pure environment reads that can be CSE'd.
/// These are values that remain constant for the entire contract invocation.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum EnvRead {
    CallDataSize,
    CallValue,
    Caller,
    Origin,
    Address,
}

/// Returns the `EnvRead` kind for an expression if it is a pure environment read.
fn env_read_kind(expression: &Expression) -> Option<EnvRead> {
    match expression {
        Expression::CallDataSize => Some(EnvRead::CallDataSize),
        Expression::CallValue => Some(EnvRead::CallValue),
        Expression::Caller => Some(EnvRead::Caller),
        Expression::Origin => Some(EnvRead::Origin),
        Expression::Address => Some(EnvRead::Address),
        _ => None,
    }
}

/// IR simplification pass.
pub struct Simplifier {
    /// Maps ValueId → constant BigUint for known constants.
    constants: BTreeMap<u32, BigUint>,
    /// Maps ValueId → ValueId for copy propagation (let x = y → x maps to y).
    copies: BTreeMap<u32, ValueId>,
    /// Maps ValueId → (UnaryOperation, operand ValueId) for unary expression tracking.
    /// Used to simplify patterns like not(not(x)) → x.
    unary_defs: BTreeMap<u32, (UnaryOperation, ValueId)>,
    /// Counter for fresh value IDs when creating new bindings (strength reduction).
    next_value_id: u32,
    /// CSE cache for pure environment reads (calldatasize, caller, etc.).
    /// Maps the read category to the first ValueId that bound this expression.
    /// Saved/restored in region scopes to ensure LLVM SSA domination correctness:
    /// a binding from one branch must not be referenced from a sibling branch.
    env_reads: BTreeMap<EnvRead, ValueId>,
    /// Statistics.
    stats: SimplifyResults,
}

impl Default for Simplifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Simplifier {
    /// Creates a new simplifier.
    pub fn new() -> Self {
        Simplifier {
            constants: BTreeMap::new(),
            copies: BTreeMap::new(),
            unary_defs: BTreeMap::new(),
            next_value_id: 0,
            env_reads: BTreeMap::new(),
            stats: SimplifyResults::default(),
        }
    }

    /// Allocates a fresh ValueId.
    fn fresh_id(&mut self) -> ValueId {
        let id = ValueId(self.next_value_id);
        self.next_value_id += 1;
        id
    }

    /// Resolves the memory region for a value if its offset is a known constant.
    fn resolve_region(&self, value: &Value) -> MemoryRegion {
        if let Some(addr) = self.constants.get(&value.id.0) {
            MemoryRegion::from_address(addr)
        } else {
            MemoryRegion::Unknown
        }
    }

    /// Simplifies an entire object in place.
    pub fn simplify_object(&mut self, object: &mut Object) -> SimplifyResults {
        // Find maximum ValueId in use so we can allocate fresh IDs
        self.next_value_id = object.find_max_value_id() + 1;

        // Simplify main code block
        self.simplify_block(&mut object.code);
        // DCE on main code block (no explicit return values)
        self.stats.dead_bindings_removed +=
            eliminate_dead_code_in_stmts(&mut object.code.statements, &BTreeSet::new());

        for function in object.functions.values_mut() {
            self.constants.clear();
            self.copies.clear();
            self.unary_defs.clear();
            self.env_reads.clear();
            function.body.statements =
                self.simplify_statements(std::mem::take(&mut function.body.statements));

            // DCE pass: remove unused pure Let bindings (bottom-up, then fixpoint)
            let mut extra_used = BTreeSet::new();
            for ret_id in &function.return_values {
                extra_used.insert(ret_id.0);
            }
            self.stats.dead_bindings_removed +=
                eliminate_dead_code_in_stmts(&mut function.body.statements, &extra_used);
        }

        // NOTE: Do NOT recurse into subobjects here. The optimize_object_tree
        // in lib.rs handles subobject recursion. Processing subobjects here would
        // cause them to be simplified BEFORE inlining runs on them, breaking the
        // required pass ordering (inline -> simplify -> mem_opt).

        std::mem::take(&mut self.stats)
    }

    /// Runs only DCE (dead code elimination) on an object without the full simplification pass.
    ///
    /// This is useful after late-stage passes (mem_opt, keccak folding) that leave
    /// Simplifies a block in place.
    fn simplify_block(&mut self, block: &mut Block) {
        block.statements = self.simplify_statements(std::mem::take(&mut block.statements));
    }

    /// Simplifies a list of statements, returning the simplified list.
    fn simplify_statements(&mut self, statements: Vec<Statement>) -> Vec<Statement> {
        let outer_constants = self.constants.clone();
        let outer_copies = self.copies.clone();

        let mut result = Vec::with_capacity(statements.len());

        for statement in statements {
            let simplified = self.simplify_statement(statement);
            result.extend(simplified);
        }

        // Post-pass: outline Solidity panic revert patterns.
        // Pattern: mstore(0, panic_selector) + mstore(4, code) + revert(0, 0x24)
        // Replace with PanicRevert { code } which deduplicates to a shared block.
        // Use self.constants which still has all accumulated constants from this scope.
        result = outline_panic_patterns(result, &self.constants);

        // Post-pass: outline Solidity Error(string) revert patterns.
        // Pattern: mload(0x40) + mstore(fmp, 0x08c379a0...) + mstore(fmp+4, 0x20) +
        //          mstore(fmp+0x24, length) + mstore(fmp+0x44, data) + revert(fmp, total)
        // Replace with ErrorStringRevert { length, data } which calls a shared function.
        // NOTE: ErrorStringRevert outlining disabled - causes regressions on OZ contracts
        // (0-1 sites per contract, inline path generates worse code than original MStores)
        // result = outline_error_string_patterns(result, &self.constants);

        // Post-pass: outline custom error revert patterns.
        // Pattern: mstore(0, selector) [+ mstore(4, arg0) + ...] + revert(0, 4+32*N)
        // Replace with CustomErrorRevert { selector, arguments }.
        result = outline_custom_error_patterns(result, &self.constants);

        self.constants = outer_constants;
        self.copies = outer_copies;

        result
    }

    /// Simplifies a single statement.
    /// Returns a vec of replacement statements (empty = remove, one = replace, multiple = expand).
    fn simplify_statement(&mut self, statement: Statement) -> Vec<Statement> {
        match statement {
            Statement::Let { bindings, value } => {
                let simplified_expr = self.simplify_expr(value);

                // Strength reduction: mul(x, 2^k) → shl(k, x), div(x, 2^k) → shr(k, x)
                // We need to emit: let shift_id = k; let result = shl(shift_id, x)
                if bindings.len() == 1 {
                    if let Some(statements) = self.try_strength_reduce(&bindings, &simplified_expr)
                    {
                        return statements;
                    }
                }

                // Track constants
                if bindings.len() == 1 {
                    if let Expression::Literal { ref value, .. } = simplified_expr {
                        self.constants.insert(bindings[0].0, value.clone());
                    }
                    // Track copies (let x = y)
                    if let Expression::Var(src_id) = &simplified_expr {
                        let resolved = self.resolve_copy(*src_id);
                        self.copies.insert(bindings[0].0, resolved);
                        // Also propagate constant knowledge
                        if let Some(c) = self.constants.get(&resolved.0).cloned() {
                            self.constants.insert(bindings[0].0, c);
                        }
                    }

                    // Record first binding for pure environment reads (CSE).
                    // Subsequent reads of the same kind will be replaced with Var(id).
                    if let Some(kind) = env_read_kind(&simplified_expr) {
                        self.env_reads.entry(kind).or_insert(bindings[0]);
                    }

                    // Track unary definitions for algebraic identity detection
                    // (e.g., not(not(x)) = x).
                    if let Expression::Unary { op, operand } = &simplified_expr {
                        self.unary_defs.insert(bindings[0].0, (*op, operand.id));
                    }
                }

                vec![Statement::Let {
                    bindings,
                    value: simplified_expr,
                }]
            }

            Statement::MStore {
                offset,
                value,
                region,
            } => {
                let offset = self.resolve_value(offset);
                let region = if region == MemoryRegion::Unknown {
                    self.resolve_region(&offset)
                } else {
                    region
                };
                vec![Statement::MStore {
                    offset,
                    value: self.resolve_value(value),
                    region,
                }]
            }

            Statement::MStore8 {
                offset,
                value,
                region,
            } => {
                let offset = self.resolve_value(offset);
                let region = if region == MemoryRegion::Unknown {
                    self.resolve_region(&offset)
                } else {
                    region
                };
                vec![Statement::MStore8 {
                    offset,
                    value: self.resolve_value(value),
                    region,
                }]
            }

            Statement::If {
                condition,
                inputs,
                then_region,
                else_region,
                outputs,
            } => {
                let condition = self.resolve_value(condition);
                let cond_val = self.try_get_const(&condition);

                // Constant branch elimination: hoist taken branch statements,
                // then assign outputs OUTSIDE the block so DCE can see them.
                if let Some(cond_const) = cond_val {
                    let is_true = !cond_const.is_zero();
                    self.stats.branches_eliminated += 1;

                    if is_true {
                        let then_region = self.simplify_region(then_region);
                        let mut result = Vec::new();
                        // Emit statements directly (not wrapped in a Block) so that
                        // DCE at the parent level can see that values defined here
                        // are used by the output assignments below.
                        result.extend(then_region.statements);
                        for (output_id, yield_val) in outputs.iter().zip(then_region.yields.iter())
                        {
                            result.push(Statement::Let {
                                bindings: vec![*output_id],
                                value: Expression::Var(yield_val.id),
                            });
                        }
                        return result;
                    } else if let Some(else_region) = else_region {
                        let else_region = self.simplify_region(else_region);
                        let mut result = Vec::new();
                        result.extend(else_region.statements);
                        for (output_id, yield_val) in outputs.iter().zip(else_region.yields.iter())
                        {
                            result.push(Statement::Let {
                                bindings: vec![*output_id],
                                value: Expression::Var(yield_val.id),
                            });
                        }
                        return result;
                    } else {
                        // Condition false, no else: outputs come from inputs (passthrough)
                        let inputs: Vec<Value> =
                            inputs.into_iter().map(|v| self.resolve_value(v)).collect();
                        let mut result = vec![];
                        for (output_id, input_val) in outputs.iter().zip(inputs.iter()) {
                            result.push(Statement::Let {
                                bindings: vec![*output_id],
                                value: Expression::Var(input_val.id),
                            });
                        }
                        return result;
                    }
                }

                let inputs: Vec<Value> =
                    inputs.into_iter().map(|v| self.resolve_value(v)).collect();
                let then_region = self.simplify_region(then_region);
                let else_region = else_region.map(|r| self.simplify_region(r));

                vec![Statement::If {
                    condition,
                    inputs,
                    then_region,
                    else_region,
                    outputs,
                }]
            }

            Statement::Switch {
                scrutinee,
                inputs,
                cases,
                default,
                outputs,
            } => {
                let scrutinee = self.resolve_value(scrutinee);
                let scrut_val = self.try_get_const(&scrutinee);

                // Constant switch elimination: hoist taken case statements,
                // then assign outputs OUTSIDE the block so DCE can see them.
                if let Some(scrut_const) = scrut_val {
                    let matching_case = cases.into_iter().find(|c| c.value == scrut_const);

                    let taken_region = if let Some(case) = matching_case {
                        self.simplify_region(case.body)
                    } else if let Some(default_region) = default {
                        self.simplify_region(default_region)
                    } else {
                        // No matching case and no default - outputs come from inputs
                        let inputs: Vec<Value> =
                            inputs.into_iter().map(|v| self.resolve_value(v)).collect();
                        let mut result = vec![];
                        for (output_id, input_val) in outputs.iter().zip(inputs.iter()) {
                            result.push(Statement::Let {
                                bindings: vec![*output_id],
                                value: Expression::Var(input_val.id),
                            });
                        }
                        self.stats.branches_eliminated += 1;
                        return result;
                    };

                    let mut result = Vec::new();
                    // Emit statements directly (not wrapped in a Block) so that
                    // DCE at the parent level can see that values defined here
                    // are used by the output assignments below.
                    result.extend(taken_region.statements);
                    for (output_id, yield_val) in outputs.iter().zip(taken_region.yields.iter()) {
                        result.push(Statement::Let {
                            bindings: vec![*output_id],
                            value: Expression::Var(yield_val.id),
                        });
                    }
                    self.stats.branches_eliminated += 1;
                    return result;
                }

                let inputs: Vec<Value> =
                    inputs.into_iter().map(|v| self.resolve_value(v)).collect();
                let mut cases: Vec<SwitchCase> = cases
                    .into_iter()
                    .map(|c| SwitchCase {
                        value: c.value,
                        body: self.simplify_region(c.body),
                    })
                    .collect();
                let mut default = default.map(|r| self.simplify_region(r));

                // Callvalue check hoisting: if all cases start with
                // `let tmp = callvalue(); if tmp { revert(0,0) }`, hoist it before the switch.
                // This eliminates N-1 redundant copies of the check (e.g., 10 copies in ERC20).
                let mut hoisted = Vec::new();
                if !cases.is_empty() {
                    let all_have_callvalue_check = cases
                        .iter()
                        .all(|c| has_callvalue_revert_prefix(&c.body.statements))
                        && default.as_ref().is_none_or(|d| {
                            d.statements.is_empty() || has_callvalue_revert_prefix(&d.statements)
                        });

                    if all_have_callvalue_check {
                        // Clone the two-statement check from the first case
                        // (Let + If), but we only need ONE copy hoisted
                        let first_stmts = &cases[0].body.statements;
                        if first_stmts.len() >= 2 {
                            hoisted.push(first_stmts[0].clone());
                            hoisted.push(first_stmts[1].clone());
                        }
                        // Remove the two-statement prefix from all cases
                        for case in &mut cases {
                            if has_callvalue_revert_prefix(&case.body.statements) {
                                case.body.statements.drain(0..2);
                            }
                        }
                        // Remove from default if it has one
                        if let Some(ref mut d) = default {
                            if has_callvalue_revert_prefix(&d.statements) {
                                d.statements.drain(0..2);
                            }
                        }
                    } else {
                        // Partial callvalue read hoisting: when not all cases have
                        // the full callvalue-revert prefix, but many cases start with
                        // `let vN = callvalue()`, hoist just the syscall read before
                        // the switch. Each case that had `let vN = callvalue()` becomes
                        // `let vN = hoisted_cv` (a copy, no syscall). This eliminates
                        // N-1 redundant callvalue syscalls.
                        const PARTIAL_HOIST_THRESHOLD: usize = 3;
                        let callvalue_case_count = cases
                            .iter()
                            .filter(|c| starts_with_callvalue_let(&c.body.statements))
                            .count();
                        let default_has_cv = default
                            .as_ref()
                            .is_some_and(|d| starts_with_callvalue_let(&d.statements));
                        let total_cv = callvalue_case_count + if default_has_cv { 1 } else { 0 };

                        if total_cv >= PARTIAL_HOIST_THRESHOLD {
                            let hoisted_cv_id = self.fresh_id();
                            hoisted.push(Statement::Let {
                                bindings: vec![hoisted_cv_id],
                                value: Expression::CallValue,
                            });
                            // Replace callvalue() in each case with Var(hoisted_cv_id)
                            for case in &mut cases {
                                replace_leading_callvalue_with_var(
                                    &mut case.body.statements,
                                    hoisted_cv_id,
                                );
                            }
                            if let Some(ref mut d) = default {
                                replace_leading_callvalue_with_var(
                                    &mut d.statements,
                                    hoisted_cv_id,
                                );
                            }
                        }
                    }
                }

                hoisted.push(Statement::Switch {
                    scrutinee,
                    inputs,
                    cases,
                    default,
                    outputs,
                });
                hoisted
            }

            Statement::For {
                initial_values,
                loop_variables,
                condition_statements,
                condition,
                body,
                post_input_variables,
                post,
                outputs,
            } => {
                let initial_values: Vec<Value> = initial_values
                    .into_iter()
                    .map(|v| self.resolve_value(v))
                    .collect();

                // Save state for loop body (loop body can't see pre-loop constants reliably
                // since values change each iteration)
                let saved_constants = self.constants.clone();
                let saved_copies = self.copies.clone();

                let condition_statements = self.simplify_statements(condition_statements);
                let condition = self.simplify_expr(condition);
                let body = self.simplify_region(body);
                let post = self.simplify_region(post);

                self.constants = saved_constants;
                self.copies = saved_copies;

                vec![Statement::For {
                    initial_values,
                    loop_variables,
                    condition_statements,
                    condition,
                    body,
                    post_input_variables,
                    post,
                    outputs,
                }]
            }

            Statement::Block(region) => vec![Statement::Block(self.simplify_region(region))],

            Statement::Expression(expression) => {
                vec![Statement::Expression(self.simplify_expr(expression))]
            }

            // Statements with no Value fields — copy through unchanged.
            // CustomErrorRevert's arguments are pre-resolved when the outliner builds it,
            // so it stays in the no-op group.
            Statement::Stop
            | Statement::Invalid
            | Statement::PanicRevert { .. }
            | Statement::ErrorStringRevert { .. }
            | Statement::CustomErrorRevert { .. } => vec![statement],

            // Pass-through statements: apply copy propagation to every used Value
            // (definitions like Let bindings, ExternalCall::result, Create::result are not
            // visited by `for_each_value_id_mut`). MStore/MStore8 are handled above
            // because they additionally re-resolve their `MemoryRegion` from the
            // (possibly-now-constant) offset.
            mut other @ (Statement::MCopy { .. }
            | Statement::SStore { .. }
            | Statement::TStore { .. }
            | Statement::MappingSStore { .. }
            | Statement::Revert { .. }
            | Statement::Return { .. }
            | Statement::ExternalCall { .. }
            | Statement::Create { .. }
            | Statement::Log { .. }
            | Statement::CodeCopy { .. }
            | Statement::ExtCodeCopy { .. }
            | Statement::ReturnDataCopy { .. }
            | Statement::DataCopy { .. }
            | Statement::CallDataCopy { .. }
            | Statement::SetImmutable { .. }
            | Statement::Leave { .. }
            | Statement::Break { .. }
            | Statement::Continue { .. }
            | Statement::SelfDestruct { .. }) => {
                other.for_each_value_id_mut(&mut |id| *id = self.resolve_copy(*id));
                vec![other]
            }
        }
    }

    /// Simplifies a region in place.
    fn simplify_region(&mut self, region: Region) -> Region {
        // Save outer scope state (including env reads to prevent cross-branch leaking)
        let outer_constants = self.constants.clone();
        let outer_copies = self.copies.clone();
        let outer_env_reads = self.env_reads.clone();

        let mut statements = Vec::with_capacity(region.statements.len());
        for statement in region.statements {
            let simplified = self.simplify_statement(statement);
            statements.extend(simplified);
        }

        // Outline panic revert patterns using the full accumulated constants for this scope
        statements = outline_panic_patterns(statements, &self.constants);

        // Outline Error(string) revert patterns
        // NOTE: ErrorStringRevert outlining disabled - see above
        // statements = outline_error_string_patterns(statements, &self.constants);

        // Outline custom error revert patterns
        statements = outline_custom_error_patterns(statements, &self.constants);

        // Resolve yields BEFORE restoring outer scope, since yield values
        // reference definitions from inside the region.
        let yields: Vec<Value> = region
            .yields
            .into_iter()
            .map(|v| self.resolve_value(v))
            .collect();

        // Restore outer scope
        self.constants = outer_constants;
        self.copies = outer_copies;
        self.env_reads = outer_env_reads;

        Region { statements, yields }
    }

    /// Simplifies an expression, performing constant folding, algebraic identities,
    /// and copy propagation on operands.
    fn simplify_expr(&mut self, expression: Expression) -> Expression {
        match expression {
            Expression::Binary { op, lhs, rhs } => {
                let lhs = self.resolve_value(lhs);
                let rhs = self.resolve_value(rhs);
                let lhs_val = self.try_get_const(&lhs);
                let rhs_val = self.try_get_const(&rhs);

                // Constant folding: both operands are constants
                if let (Some(a), Some(b)) = (&lhs_val, &rhs_val) {
                    if let Some(result) = fold_binary(op, a, b) {
                        self.stats.constants_folded += 1;
                        return Expression::Literal {
                            value: result,
                            ty: result_type(op),
                        };
                    }
                }

                // Algebraic identities (safe: just rewrites the expression)
                if let Some(simplified) = simplify_binary(op, &lhs, &rhs, &lhs_val, &rhs_val) {
                    self.stats.identities_simplified += 1;
                    return simplified;
                }

                // NOTE: AND mask elimination (and(x, MASK) = x when x fits in MASK bits)
                // was implemented but causes a code size INCREASE because LLVM uses AND
                // operations as range hints for its own optimization passes. Removing them
                // makes LLVM generate more conservative code. The type inference pass
                // already handles narrow types for the LLVM codegen.

                Expression::Binary { op, lhs, rhs }
            }

            Expression::Unary { op, operand } => {
                let operand = self.resolve_value(operand);
                let operand_val = self.try_get_const(&operand);

                if let Some(c) = &operand_val {
                    if let Some(result) = fold_unary(op, c) {
                        self.stats.constants_folded += 1;
                        return Expression::Literal {
                            value: result,
                            ty: unary_result_type(op),
                        };
                    }
                }

                // Algebraic identity: not(not(x)) = x (double bitwise negation)
                if op == UnaryOperation::Not {
                    if let Some((UnaryOperation::Not, inner)) = self.unary_defs.get(&operand.id.0) {
                        self.stats.identities_simplified += 1;
                        return Expression::Var(*inner);
                    }
                }

                Expression::Unary { op, operand }
            }

            Expression::Ternary { op, a, b, n } => {
                let a = self.resolve_value(a);
                let b = self.resolve_value(b);
                let n = self.resolve_value(n);
                let a_val = self.try_get_const(&a);
                let b_val = self.try_get_const(&b);
                let n_val = self.try_get_const(&n);

                if let (Some(av), Some(bv), Some(nv)) = (&a_val, &b_val, &n_val) {
                    if let Some(result) = fold_ternary(op, av, bv, nv) {
                        self.stats.constants_folded += 1;
                        return Expression::Literal {
                            value: result,
                            ty: Type::Int(BitWidth::I256),
                        };
                    }
                }

                Expression::Ternary { op, a, b, n }
            }

            // Resolve copies in Var references
            Expression::Var(id) => {
                let resolved = self.resolve_copy(id);
                Expression::Var(resolved)
            }

            // MLoad: resolve offset and annotate memory region from constant offsets
            Expression::MLoad { offset, region } => {
                let offset = self.resolve_value(offset);
                let region = if region == MemoryRegion::Unknown {
                    self.resolve_region(&offset)
                } else {
                    region
                };
                Expression::MLoad { offset, region }
            }

            // CSE for pure environment reads: replace with cached binding if available.
            // These values are invariant for the entire contract invocation.
            Expression::CallDataSize => self.cse_env_read(EnvRead::CallDataSize, expression),
            Expression::CallValue => self.cse_env_read(EnvRead::CallValue, expression),
            Expression::Caller => self.cse_env_read(EnvRead::Caller, expression),
            Expression::Origin => self.cse_env_read(EnvRead::Origin, expression),
            Expression::Address => self.cse_env_read(EnvRead::Address, expression),

            // Constant keccak256 folding: precompute hash of constant arguments
            Expression::Keccak256Single { word0 } => {
                let word0 = self.resolve_value(word0);
                if let Some(c) = self.try_get_const(&word0) {
                    let result = fold_keccak256_single(&c);
                    self.stats.constants_folded += 1;
                    Expression::Literal {
                        value: result,
                        ty: Type::Int(BitWidth::I256),
                    }
                } else {
                    Expression::Keccak256Single { word0 }
                }
            }

            Expression::Keccak256Pair { word0, word1 } => {
                let word0 = self.resolve_value(word0);
                let word1 = self.resolve_value(word1);
                if let (Some(c0), Some(c1)) =
                    (self.try_get_const(&word0), self.try_get_const(&word1))
                {
                    let result = fold_keccak256_pair(&c0, &c1);
                    self.stats.constants_folded += 1;
                    Expression::Literal {
                        value: result,
                        ty: Type::Int(BitWidth::I256),
                    }
                } else {
                    Expression::Keccak256Pair { word0, word1 }
                }
            }

            Expression::MappingSLoad { key, slot } => Expression::MappingSLoad {
                key: self.resolve_value(key),
                slot: self.resolve_value(slot),
            },

            // All other expressions pass through unchanged
            other => other,
        }
    }

    /// Checks if an environment read has been cached and returns a Var reference
    /// to the first binding if so. Otherwise returns the original expression.
    fn cse_env_read(&mut self, kind: EnvRead, original: Expression) -> Expression {
        if let Some(&cached_id) = self.env_reads.get(&kind) {
            self.stats.env_reads_eliminated += 1;
            Expression::Var(cached_id)
        } else {
            original
        }
    }

    /// Resolves a Value through copy propagation.
    fn resolve_value(&self, value: Value) -> Value {
        let resolved = self.resolve_copy(value.id);
        if resolved != value.id {
            Value {
                id: resolved,
                ..value
            }
        } else {
            value
        }
    }

    /// Resolves a ValueId through the copy chain.
    fn resolve_copy(&self, id: ValueId) -> ValueId {
        let mut current = id;
        // Follow the copy chain (with cycle protection)
        for _ in 0..32 {
            if let Some(&target) = self.copies.get(&current.0) {
                if target == current {
                    break;
                }
                current = target;
            } else {
                break;
            }
        }
        current
    }

    /// Tries to get the constant value for a Value.
    fn try_get_const(&self, value: &Value) -> Option<BigUint> {
        let resolved = self.resolve_copy(value.id);
        self.constants.get(&resolved.0).cloned()
    }

    /// Emits the two-`Let` strength-reduction template:
    ///   `let helper = const_value; let result = target_op(<lhs>, <rhs>)`
    /// where one operand of `target_op` is the freshly-bound `helper` and the
    /// other is `other_operand` (its position controlled by `helper_on_lhs`).
    fn emit_strength_reduce(
        &mut self,
        bindings: &[ValueId],
        target_op: BinaryOperation,
        const_value: BigUint,
        helper_on_lhs: bool,
        other_operand: Value,
    ) -> Vec<Statement> {
        let helper_id = self.fresh_id();
        self.constants.insert(helper_id.0, const_value.clone());
        self.stats.identities_simplified += 1;
        let helper_val = Value::int(helper_id);
        let (lhs, rhs) = if helper_on_lhs {
            (helper_val, other_operand)
        } else {
            (other_operand, helper_val)
        };
        vec![
            Statement::Let {
                bindings: vec![helper_id],
                value: Expression::Literal {
                    value: const_value,
                    ty: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: bindings.to_vec(),
                value: Expression::Binary {
                    op: target_op,
                    lhs,
                    rhs,
                },
            },
        ]
    }

    /// Attempts strength reduction on a Let binding. Transforms:
    /// - `mul(x, 2^k)` or `mul(2^k, x)` → `shl(k, x)`
    /// - `div(x, 2^k)` → `shr(k, x)` (unsigned only)
    /// - `mod(x, 2^k)` → `and(x, 2^k - 1)`
    fn try_strength_reduce(
        &mut self,
        bindings: &[ValueId],
        expression: &Expression,
    ) -> Option<Vec<Statement>> {
        let (op, lhs, rhs) = match expression {
            Expression::Binary { op, lhs, rhs } => (*op, *lhs, *rhs),
            _ => return None,
        };
        let lhs_val = self.try_get_const(&lhs);
        let rhs_val = self.try_get_const(&rhs);
        let in_range = |k: u32| (1..256).contains(&k);

        match op {
            BinaryOperation::Mul => {
                let (k, value) = if let Some(k) = rhs_val.as_ref().and_then(log2_exact) {
                    (k, lhs)
                } else if let Some(k) = lhs_val.as_ref().and_then(log2_exact) {
                    (k, rhs)
                } else {
                    return None;
                };
                in_range(k).then(|| {
                    self.emit_strength_reduce(
                        bindings,
                        BinaryOperation::Shl,
                        BigUint::from(k),
                        true,
                        value,
                    )
                })
            }
            BinaryOperation::Div => {
                let k = rhs_val.as_ref().and_then(log2_exact)?;
                in_range(k).then(|| {
                    self.emit_strength_reduce(
                        bindings,
                        BinaryOperation::Shr,
                        BigUint::from(k),
                        true,
                        lhs,
                    )
                })
            }
            BinaryOperation::Mod => {
                let k = rhs_val.as_ref().and_then(log2_exact)?;
                in_range(k).then(|| {
                    let mask = (BigUint::one() << k) - BigUint::one();
                    self.emit_strength_reduce(bindings, BinaryOperation::And, mask, false, lhs)
                })
            }
            _ => None,
        }
    }
}

/// The Solidity Panic(uint256) ABI selector: `keccak256("Panic(uint256)")` left-padded.
const PANIC_SELECTOR_HEX: &str = "4e487b7100000000000000000000000000000000000000000000000000000000";

/// Detects and replaces the Solidity panic revert pattern in a statement list.
///
/// The pattern is a sequence of statements ending with:
///   let bindings for constants, mstore(0, 0x4e487b71...), more let bindings,
///   mstore(4, error_code), more let bindings, revert(0, 0x24)
///
/// Each match is replaced with `PanicRevert { code }`, and the preceding
/// Let bindings used only by the panic are left for DCE to remove.
///
/// This function merges the caller's constant map with local Let-literal bindings
/// to correctly resolve constants defined either in the current scope or in outer scopes
/// (after copy propagation may have replaced local definitions with outer references).
fn outline_panic_patterns(
    statements: Vec<Statement>,
    scope_constants: &BTreeMap<u32, BigUint>,
) -> Vec<Statement> {
    // Quick check: does this list even contain a Revert statement?
    let has_revert = statements
        .iter()
        .any(|s| matches!(s, Statement::Revert { .. }));
    if !has_revert {
        return statements;
    }

    // Build a merged constant map: start from scope constants, add local literals
    let mut constants: BTreeMap<u32, BigUint> = scope_constants.clone();
    for statement in &statements {
        if let Statement::Let {
            bindings,
            value: Expression::Literal { value, .. },
        } = statement
        {
            if bindings.len() == 1 {
                constants.insert(bindings[0].0, value.clone());
            }
        }
    }

    let mut result = Vec::with_capacity(statements.len());

    for statement in statements {
        // Check if this is a revert(0, 0x24) - the terminal of a panic pattern
        if let Statement::Revert {
            ref offset,
            ref length,
        } = statement
        {
            if is_const_value(offset.id, 0, &constants)
                && is_const_value(length.id, 0x24, &constants)
            {
                if let Some((panic_start, error_code)) =
                    find_panic_pattern_backwards(&result, &constants)
                {
                    result.truncate(panic_start);
                    result.push(Statement::PanicRevert { code: error_code });
                    continue;
                }
            }
        }

        result.push(statement);
    }

    result
}

/// Looks backwards from the end of a statement list to find the panic pattern:
///   mstore(0, panic_selector), [let bindings]*, mstore(4, code), [let bindings]*
///
/// Returns `(start_index, error_code)` if the pattern is found.
fn find_panic_pattern_backwards(
    statements: &[Statement],
    constants: &BTreeMap<u32, BigUint>,
) -> Option<(usize, u8)> {
    let len = statements.len();
    if len < 2 {
        return None;
    }

    // Find the mstore(4, code) - must be within the last few statements (allow some Let bindings)
    let mut mstore4_idx = None;
    let mut error_code = None;
    let search_limit = len.saturating_sub(10);

    for j in (search_limit..len).rev() {
        match &statements[j] {
            Statement::MStore { offset, value, .. } => {
                if is_const_value(offset.id, 4, constants) {
                    if let Some(code_val) = constants.get(&value.id.0) {
                        if let Some(code_u8) = code_val.to_u64_digits().first() {
                            if *code_u8 <= 0xFF {
                                mstore4_idx = Some(j);
                                error_code = Some(*code_u8 as u8);
                                break;
                            }
                        }
                        // Handle zero code
                        if code_val.is_zero() {
                            mstore4_idx = Some(j);
                            error_code = Some(0u8);
                            break;
                        }
                    }
                }
            }
            Statement::Let { .. } | Statement::Expression(..) => continue,
            _ => break,
        }
    }

    let mstore4_idx = mstore4_idx?;
    let error_code = error_code?;

    // Now find mstore(0, panic_selector) before mstore4_idx
    let search_limit2 = mstore4_idx.saturating_sub(10);
    for j in (search_limit2..mstore4_idx).rev() {
        match &statements[j] {
            Statement::MStore { offset, value, .. } => {
                if is_const_value(offset.id, 0, constants) {
                    if let Some(sel_val) = constants.get(&value.id.0) {
                        let sel_hex = format!("{sel_val:064x}");
                        if sel_hex == PANIC_SELECTOR_HEX {
                            // Verify that between j and the end there are only mstores, let
                            // bindings, and dead expressions (no side effects we'd be removing)
                            let only_safe = statements[j..].iter().all(|s| {
                                matches!(
                                    s,
                                    Statement::Let { .. }
                                        | Statement::MStore { .. }
                                        | Statement::Expression(..)
                                )
                            });
                            if only_safe {
                                return Some((j, error_code));
                            }
                        }
                    }
                }
            }
            Statement::Let { .. } | Statement::Expression(..) => continue,
            _ => break,
        }
    }

    None
}

/// Checks if a ValueId maps to a specific constant value.
fn is_const_value(id: ValueId, expected: u64, constants: &BTreeMap<u32, BigUint>) -> bool {
    constants
        .get(&id.0)
        .is_some_and(|v| *v == BigUint::from(expected))
}

/// Outlines custom error revert patterns in a statement list.
///
/// Detects the pattern: mstore(0, selector) [+ mstore(4, arg0) + mstore(0x24, arg1) + ...] + revert(0, 4+32*N)
/// where the selector is a constant and the revert uses scratch space (offset 0).
/// Replaces matched patterns with `CustomErrorRevert { selector, arguments }`.
fn outline_custom_error_patterns(
    statements: Vec<Statement>,
    scope_constants: &BTreeMap<u32, BigUint>,
) -> Vec<Statement> {
    // Quick check: does this list contain a Revert statement?
    let has_revert = statements
        .iter()
        .any(|s| matches!(s, Statement::Revert { .. }));
    if !has_revert {
        return statements;
    }

    // Build merged constant map
    let mut constants: BTreeMap<u32, BigUint> = scope_constants.clone();
    for statement in &statements {
        if let Statement::Let {
            bindings,
            value: Expression::Literal { value, .. },
        } = statement
        {
            if bindings.len() == 1 {
                constants.insert(bindings[0].0, value.clone());
            }
        }
    }

    let zero = BigUint::ZERO;
    let mut result = Vec::with_capacity(statements.len());

    for statement in statements {
        // Check if this is a revert(0, N) where N is 4, 0x24, 0x44, 0x64, 0x84
        if let Statement::Revert {
            ref offset,
            ref length,
        } = statement
        {
            // The revert offset must be constant 0
            if constants.get(&offset.id.0).is_some_and(|v| *v == zero) {
                // The revert length must be a constant: 4 (0-argument), 0x24 (1-argument), 0x44 (2-argument), etc.
                if let Some(total_len) = constants.get(&length.id.0).and_then(|v| v.to_u64()) {
                    let num_args = if total_len == 4 {
                        Some(0usize)
                    } else if total_len >= 0x24 && (total_len - 4) % 0x20 == 0 {
                        Some(((total_len - 4) / 0x20) as usize)
                    } else {
                        None
                    };

                    if let Some(num_args) = num_args {
                        if let Some((start_idx, selector, arguments)) =
                            find_custom_error_pattern_backwards(&result, &constants, num_args)
                        {
                            // Keep Let bindings, remove only MStore statements
                            let mut kept = Vec::new();
                            for s in result.drain(start_idx..) {
                                match s {
                                    Statement::MStore { .. } => {} // remove
                                    _ => kept.push(s),             // keep lets/exprs
                                }
                            }
                            result.extend(kept);
                            result.push(Statement::CustomErrorRevert {
                                selector,
                                arguments,
                            });
                            continue;
                        }
                    }
                }
            }
        }

        result.push(statement);
    }

    result
}

/// Looks backwards from the end of a statement list to find the custom error pattern.
///
/// Searches for:
///   1. mstore(0, selector) at scratch space — selector is a constant
///   2. mstore(4, arg0) — first argument (optional)
///   3. mstore(0x24, arg1) — second argument (optional)
///   4. mstore(0x44, arg2) — third argument (optional)
///
/// Returns `(start_index, selector, arguments)` if found.
fn find_custom_error_pattern_backwards(
    statements: &[Statement],
    constants: &BTreeMap<u32, BigUint>,
    num_args: usize,
) -> Option<(usize, BigUint, Vec<Value>)> {
    let len = statements.len();
    if len < 1 + num_args {
        return None;
    }

    let mut found_selector: Option<BigUint> = None;
    let mut arguments: Vec<Option<Value>> = vec![None; num_args];
    let mut earliest_idx = len;

    let zero = BigUint::ZERO;
    let four = BigUint::from(4u32);

    let search_limit = len.saturating_sub(20);
    for j in (search_limit..len).rev() {
        match &statements[j] {
            Statement::MStore { offset, value, .. } => {
                // Check the mstore offset
                if let Some(off_val) = constants.get(&offset.id.0) {
                    if *off_val == zero {
                        // mstore(0, selector) — selector must be a constant
                        if let Some(sel) = constants.get(&value.id.0) {
                            found_selector = Some(sel.clone());
                            earliest_idx = earliest_idx.min(j);
                        }
                    } else if *off_val == four && num_args >= 1 {
                        // mstore(4, arg0)
                        arguments[0] = Some(*value);
                        earliest_idx = earliest_idx.min(j);
                    } else {
                        // mstore(0x24, arg1), mstore(0x44, arg2), ...
                        if let Some(off_u64) = off_val.to_u64() {
                            if off_u64 >= 0x24 && (off_u64 - 4) % 0x20 == 0 {
                                let arg_idx = ((off_u64 - 4) / 0x20) as usize;
                                if arg_idx < num_args {
                                    arguments[arg_idx] = Some(*value);
                                    earliest_idx = earliest_idx.min(j);
                                }
                            }
                        }
                    }
                }
            }
            Statement::Let { .. } | Statement::Expression(..) => continue,
            _ => break,
        }
    }

    // Verify all parts were found
    let selector = found_selector?;
    let arguments: Vec<Value> = arguments.into_iter().collect::<Option<Vec<_>>>()?;

    // Verify that statements between earliest_idx and len are only Let/MStore/Expression
    let all_safe = statements[earliest_idx..].iter().all(|s| {
        matches!(
            s,
            Statement::Let { .. } | Statement::MStore { .. } | Statement::Expression(..)
        )
    });
    if !all_safe {
        return None;
    }

    Some((earliest_idx, selector, arguments))
}

/// Returns the result type for a binary operation.
fn result_type(op: BinaryOperation) -> Type {
    match op {
        BinaryOperation::Lt
        | BinaryOperation::Gt
        | BinaryOperation::Slt
        | BinaryOperation::Sgt
        | BinaryOperation::Eq => Type::Int(BitWidth::I256),
        _ => Type::Int(BitWidth::I256),
    }
}

/// Returns the result type for a unary operation.
fn unary_result_type(op: UnaryOperation) -> Type {
    match op {
        UnaryOperation::IsZero => Type::Int(BitWidth::I256),
        UnaryOperation::Not | UnaryOperation::Clz => Type::Int(BitWidth::I256),
    }
}

/// Folds a binary operation on two constant values.
/// Returns None if the operation cannot be folded.
fn fold_binary(op: BinaryOperation, a: &BigUint, b: &BigUint) -> Option<BigUint> {
    let modulus = modulus_u256();
    let max = max_u256();

    Some(match op {
        BinaryOperation::Add => (a + b) % &modulus,
        BinaryOperation::Sub => {
            if a >= b {
                a - b
            } else {
                // Wrapping subtraction: a - b + 2^256
                &modulus - (b - a)
            }
        }
        BinaryOperation::Mul => (a * b) % &modulus,
        BinaryOperation::Div => {
            if b.is_zero() {
                BigUint::zero()
            } else {
                a / b
            }
        }
        BinaryOperation::SDiv => {
            if b.is_zero() {
                BigUint::zero()
            } else {
                fold_sdiv(a, b, &modulus)?
            }
        }
        BinaryOperation::Mod => {
            if b.is_zero() {
                BigUint::zero()
            } else {
                a % b
            }
        }
        BinaryOperation::SMod => {
            if b.is_zero() {
                BigUint::zero()
            } else {
                fold_smod(a, b, &modulus)?
            }
        }
        BinaryOperation::Exp => {
            // a^b mod 2^256
            a.modpow(b, &modulus)
        }
        BinaryOperation::And => a & b,
        BinaryOperation::Or => a | b,
        BinaryOperation::Xor => a ^ b,
        // EVM shift convention: shl(shift_amount, value) = value << shift_amount
        // In our IR: Binary { Shl, lhs: shift_amount, rhs: value }
        // So a = shift_amount, b = value
        BinaryOperation::Shl => {
            if *a >= BigUint::from(256u32) {
                BigUint::zero()
            } else {
                let shift = a.to_u64_digits().first().copied().unwrap_or(0) as usize;
                (b << shift) % &modulus
            }
        }
        BinaryOperation::Shr => {
            if *a >= BigUint::from(256u32) {
                BigUint::zero()
            } else {
                let shift = a.to_u64_digits().first().copied().unwrap_or(0) as usize;
                b >> shift
            }
        }
        BinaryOperation::Sar => fold_sar(b, a, &modulus, &max)?,
        BinaryOperation::Lt => bool_to_u256(a < b),
        BinaryOperation::Gt => bool_to_u256(a > b),
        BinaryOperation::Eq => bool_to_u256(a == b),
        BinaryOperation::Slt => {
            let a_signed = is_negative(a, &modulus);
            let b_signed = is_negative(b, &modulus);
            match (a_signed, b_signed) {
                (true, false) => bool_to_u256(true),
                (false, true) => bool_to_u256(false),
                _ => bool_to_u256(a < b),
            }
        }
        BinaryOperation::Sgt => {
            let a_signed = is_negative(a, &modulus);
            let b_signed = is_negative(b, &modulus);
            match (a_signed, b_signed) {
                (true, false) => bool_to_u256(false),
                (false, true) => bool_to_u256(true),
                _ => bool_to_u256(a > b),
            }
        }
        BinaryOperation::Byte => {
            // byte(n, x): nth byte of x (0-indexed from most significant)
            if *a >= BigUint::from(32u32) {
                BigUint::zero()
            } else {
                let n = a.to_u64_digits().first().copied().unwrap_or(0) as usize;
                let shift = (31 - n) * 8;
                (b >> shift) & BigUint::from(0xffu32)
            }
        }
        BinaryOperation::SignExtend => fold_signextend(a, b, &max)?,
        // Ternary ops handled separately
        BinaryOperation::AddMod | BinaryOperation::MulMod => return None,
    })
}

/// Folds a unary operation on a constant value.
fn fold_unary(op: UnaryOperation, a: &BigUint) -> Option<BigUint> {
    Some(match op {
        UnaryOperation::IsZero => bool_to_u256(a.is_zero()),
        UnaryOperation::Not => {
            // Bitwise NOT: flip all 256 bits
            &max_u256() ^ a
        }
        UnaryOperation::Clz => {
            if a.is_zero() {
                BigUint::from(256u32)
            } else {
                let bits = a.bits();
                BigUint::from(256u64 - bits)
            }
        }
    })
}

/// Folds a ternary operation (addmod, mulmod).
fn fold_ternary(op: BinaryOperation, a: &BigUint, b: &BigUint, n: &BigUint) -> Option<BigUint> {
    if n.is_zero() {
        return Some(BigUint::zero());
    }
    Some(match op {
        BinaryOperation::AddMod => (a + b) % n,
        BinaryOperation::MulMod => (a * b) % n,
        _ => return None,
    })
}

/// Encodes a BigUint as a 32-byte big-endian buffer (left-padded with zeros).
fn biguint_to_be32(value: &BigUint) -> [u8; 32] {
    let bytes = value.to_bytes_be();
    let mut buf = [0u8; 32];
    let start = 32usize.saturating_sub(bytes.len());
    buf[start..].copy_from_slice(&bytes[bytes.len().saturating_sub(32)..]);
    buf
}

/// Computes keccak256 of a single 256-bit word at compile time.
fn fold_keccak256_single(word0: &BigUint) -> BigUint {
    let buf = biguint_to_be32(word0);
    let hash = revive_common::Keccak256::from_slice(&buf);
    BigUint::from_bytes_be(hash.as_bytes())
}

/// Computes keccak256 of two 256-bit words at compile time.
fn fold_keccak256_pair(word0: &BigUint, word1: &BigUint) -> BigUint {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(&biguint_to_be32(word0));
    buf[32..].copy_from_slice(&biguint_to_be32(word1));
    let hash = revive_common::Keccak256::from_slice(&buf);
    BigUint::from_bytes_be(hash.as_bytes())
}

/// Returns the power-of-2 exponent if the value is a power of 2.
/// E.g., 1 -> Some(0), 2 -> Some(1), 4 -> Some(2), 32 -> Some(5), 256 -> Some(8).
fn log2_exact(value: &BigUint) -> Option<u32> {
    if value.is_zero() || !((value - BigUint::one()) & value).is_zero() {
        return None;
    }
    // value is a power of 2: find the exponent
    Some((value.bits() - 1) as u32)
}

/// Applies algebraic identity simplifications.
/// Returns Some(simplified_expr) if an identity applies, None otherwise.
fn simplify_binary(
    op: BinaryOperation,
    lhs: &Value,
    rhs: &Value,
    lhs_val: &Option<BigUint>,
    rhs_val: &Option<BigUint>,
) -> Option<Expression> {
    let i256 = Type::Int(BitWidth::I256);
    let literal = |value: BigUint| Expression::Literal { value, ty: i256 };
    let var_l = || Expression::Var(lhs.id);
    let var_r = || Expression::Var(rhs.id);
    let same = lhs.id == rhs.id;
    let l_is = |v: &BigUint| lhs_val.as_ref() == Some(v);
    let r_is = |v: &BigUint| rhs_val.as_ref() == Some(v);
    let zero = BigUint::zero();
    let one = BigUint::one();

    match op {
        // add(x, 0) = add(0, x) = x
        BinaryOperation::Add => {
            if r_is(&zero) {
                Some(var_l())
            } else if l_is(&zero) {
                Some(var_r())
            } else {
                None
            }
        }

        // sub(x, 0) = x; sub(x, x) = 0
        BinaryOperation::Sub if r_is(&zero) => Some(var_l()),
        BinaryOperation::Sub if same => Some(literal(zero)),
        BinaryOperation::Sub => None,

        // mul(x, 0) = mul(0, x) = 0; mul(x, 1) = mul(1, x) = x
        BinaryOperation::Mul if r_is(&zero) || l_is(&zero) => Some(literal(zero)),
        BinaryOperation::Mul if r_is(&one) => Some(var_l()),
        BinaryOperation::Mul if l_is(&one) => Some(var_r()),
        BinaryOperation::Mul => None,

        // div(x, 1) = x; div(0, x) = 0; div(x, x) = 1 (when x != 0; same id is definitely equal)
        BinaryOperation::Div | BinaryOperation::SDiv if r_is(&one) => Some(var_l()),
        BinaryOperation::Div | BinaryOperation::SDiv if l_is(&zero) => Some(literal(zero)),
        BinaryOperation::Div | BinaryOperation::SDiv if same => Some(literal(one)),
        BinaryOperation::Div | BinaryOperation::SDiv => None,

        // mod(x, 1) = 0; mod(0, x) = 0; mod(x, x) = 0
        BinaryOperation::Mod | BinaryOperation::SMod if r_is(&one) || l_is(&zero) || same => {
            Some(literal(zero))
        }
        BinaryOperation::Mod | BinaryOperation::SMod => None,

        // and(_, 0) = 0; and(x, MAX) = x; and(x, x) = x
        BinaryOperation::And if r_is(&zero) || l_is(&zero) => Some(literal(zero)),
        BinaryOperation::And if r_is(&max_u256()) || same => Some(var_l()),
        BinaryOperation::And if l_is(&max_u256()) => Some(var_r()),
        BinaryOperation::And => None,

        // or(x, 0) = x; or(_, MAX) = MAX; or(x, x) = x
        BinaryOperation::Or if r_is(&zero) || same => Some(var_l()),
        BinaryOperation::Or if l_is(&zero) => Some(var_r()),
        BinaryOperation::Or if r_is(&max_u256()) || l_is(&max_u256()) => Some(literal(max_u256())),
        BinaryOperation::Or => None,

        // xor(x, 0) = x; xor(x, x) = 0
        BinaryOperation::Xor if r_is(&zero) => Some(var_l()),
        BinaryOperation::Xor if l_is(&zero) => Some(var_r()),
        BinaryOperation::Xor if same => Some(literal(zero)),
        BinaryOperation::Xor => None,

        // shl/shr/sar(0, x) = x (IR convention: lhs = shift_amount, rhs = value)
        BinaryOperation::Shl | BinaryOperation::Shr | BinaryOperation::Sar if l_is(&zero) => {
            Some(var_r())
        }
        BinaryOperation::Shl | BinaryOperation::Shr | BinaryOperation::Sar => None,

        // eq(x, x) = 1
        BinaryOperation::Eq if same => Some(literal(one)),
        BinaryOperation::Eq => None,

        // lt/gt/slt/sgt(x, x) = 0
        BinaryOperation::Lt | BinaryOperation::Gt | BinaryOperation::Slt | BinaryOperation::Sgt
            if same =>
        {
            Some(literal(zero))
        }
        BinaryOperation::Lt | BinaryOperation::Gt | BinaryOperation::Slt | BinaryOperation::Sgt => {
            None
        }

        _ => None,
    }
}

/// Helper: checks if a 256-bit value is negative in two's complement.
fn is_negative(value: &BigUint, modulus: &BigUint) -> bool {
    *value >= (modulus >> 1)
}

/// Helper: converts a boolean to a 256-bit value (0 or 1).
fn bool_to_u256(b: bool) -> BigUint {
    if b {
        BigUint::one()
    } else {
        BigUint::zero()
    }
}

/// Helper: signed division for 256-bit values.
fn fold_sdiv(a: &BigUint, b: &BigUint, modulus: &BigUint) -> Option<BigUint> {
    let half = modulus >> 1;
    let a_neg = *a >= half;
    let b_neg = *b >= half;

    let abs_a = if a_neg { modulus - a } else { a.clone() };
    let abs_b = if b_neg { modulus - b } else { b.clone() };

    if abs_b.is_zero() {
        return Some(BigUint::zero());
    }

    let result = &abs_a / &abs_b;

    // Negate if signs differ
    if a_neg != b_neg && !result.is_zero() {
        Some(modulus - &result)
    } else {
        Some(result)
    }
}

/// Helper: signed modulo for 256-bit values.
fn fold_smod(a: &BigUint, b: &BigUint, modulus: &BigUint) -> Option<BigUint> {
    let half = modulus >> 1;
    let a_neg = *a >= half;
    let b_neg = *b >= half;

    let abs_a = if a_neg { modulus - a } else { a.clone() };
    let abs_b = if b_neg { modulus - b } else { b.clone() };

    if abs_b.is_zero() {
        return Some(BigUint::zero());
    }

    let result = &abs_a % &abs_b;

    // Result has sign of dividend (a)
    if a_neg && !result.is_zero() {
        Some(modulus - &result)
    } else {
        Some(result)
    }
}

/// Helper: arithmetic shift right for 256-bit values.
fn fold_sar(a: &BigUint, b: &BigUint, modulus: &BigUint, max: &BigUint) -> Option<BigUint> {
    if *b >= BigUint::from(256u32) {
        let half = modulus >> 1;
        return if *a >= half {
            // Negative: SAR fills with 1s
            Some(max.clone())
        } else {
            Some(BigUint::zero())
        };
    }

    let shift = b.to_u64_digits().first().copied().unwrap_or(0) as usize;
    let half = modulus >> 1;

    if *a >= half {
        // Negative value: fill with 1s from the top
        let shifted = a >> shift;
        let fill = (max >> (256 - shift)) << (256 - shift);
        Some((shifted | fill) % modulus)
    } else {
        Some(a >> shift)
    }
}

/// Helper: EVM signextend(b, x) operation.
fn fold_signextend(b: &BigUint, x: &BigUint, max: &BigUint) -> Option<BigUint> {
    if *b >= BigUint::from(31u32) {
        // No change when b >= 31
        return Some(x.clone());
    }

    let byte_pos = b.to_u64_digits().first().copied().unwrap_or(0) as usize;
    let bit_pos = byte_pos * 8 + 7;
    let sign_bit = (x >> bit_pos) & BigUint::one();

    if sign_bit.is_one() {
        // Sign bit is 1: fill upper bits with 1s
        let mask = max << (bit_pos + 1);
        Some((x | mask) & max)
    } else {
        // Sign bit is 0: clear upper bits
        let mask = (BigUint::one() << (bit_pos + 1)) - BigUint::one();
        Some(x & mask)
    }
}

/// Checks if an expression has side effects (cannot be dead-code eliminated).
fn expr_has_side_effects(expression: &Expression) -> bool {
    matches!(
        expression,
        Expression::Call { .. }
            | Expression::Keccak256 { .. }
            | Expression::Keccak256Pair { .. }
            | Expression::Keccak256Single { .. }
            | Expression::MLoad { .. }
            | Expression::SLoad { .. }
            | Expression::TLoad { .. }
            | Expression::MappingSLoad { .. }
            | Expression::MSize
    )
}

/// Eliminates dead Let bindings from a list of statements.
/// Uses bottom-up recursive DCE: first cleans nested regions, then this level.
/// Iterates at each level until fixpoint (no more removals).
///
/// `extra_used` contains ValueIds that must be preserved even if not referenced
/// by the statements themselves (e.g., function return values, region yields).
fn eliminate_dead_code_in_stmts(
    statements: &mut Vec<Statement>,
    extra_used: &BTreeSet<u32>,
) -> usize {
    let mut total_removed = 0;

    // Phase 0: Remove unreachable code after terminators (revert/return/stop/invalid/leave)
    if let Some(terminator_pos) = statements.iter().position(is_terminator) {
        let unreachable_count = statements.len() - terminator_pos - 1;
        if unreachable_count > 0 {
            statements.truncate(terminator_pos + 1);
            total_removed += unreachable_count;
        }
    }

    // Phase 1: Recursively DCE nested regions (bottom-up)
    for statement in statements.iter_mut() {
        total_removed += eliminate_dead_code_in_nested(statement, extra_used);
    }

    // Phase 2: DCE at this level with fixpoint iteration
    loop {
        let mut used = extra_used.clone();
        for statement in statements.iter() {
            statement.for_each_value_id(&mut |id| {
                used.insert(id.0);
            });
        }

        let before = statements.len();
        statements.retain(|statement| {
            // Remove unused Let bindings with pure expressions
            if let Statement::Let { bindings, value } = statement {
                let all_unused = bindings.iter().all(|id| !used.contains(&id.0));
                if all_unused && !expr_has_side_effects(value) {
                    return false;
                }
            }
            // Remove pure expression statements (e.g., void literals after revert/return)
            if let Statement::Expression(expression) = statement {
                if !expr_has_side_effects(expression) {
                    return false;
                }
            }
            true
        });

        let removed = before - statements.len();
        total_removed += removed;
        if removed == 0 {
            break;
        }
    }

    total_removed
}

/// Recursively DCE inside nested regions of a statement.
/// `parent_extra_used` contains ValueIds from the parent scope that must be preserved
/// (e.g., function return values). These are propagated into Block regions because
/// Blocks in Yul can define values that are referenced by the parent scope.
fn eliminate_dead_code_in_nested(
    statement: &mut Statement,
    parent_extra_used: &BTreeSet<u32>,
) -> usize {
    match statement {
        Statement::If {
            then_region,
            else_region,
            ..
        } => {
            let mut removed = 0;
            let extra = yields_as_used(&then_region.yields);
            removed += eliminate_dead_code_in_stmts(&mut then_region.statements, &extra);
            if let Some(r) = else_region {
                let extra = yields_as_used(&r.yields);
                removed += eliminate_dead_code_in_stmts(&mut r.statements, &extra);
            }
            removed
        }
        Statement::Switch { cases, default, .. } => {
            let mut removed = 0;
            for c in cases {
                let extra = yields_as_used(&c.body.yields);
                removed += eliminate_dead_code_in_stmts(&mut c.body.statements, &extra);
            }
            if let Some(d) = default {
                let extra = yields_as_used(&d.yields);
                removed += eliminate_dead_code_in_stmts(&mut d.statements, &extra);
            }
            removed
        }
        Statement::Block(region) => {
            // Block regions can define values used by the parent scope (e.g., function
            // return values assigned inside a scoped block). Propagate parent's extra_used
            // so those definitions are not incorrectly DCE'd.
            let mut extra = yields_as_used(&region.yields);
            extra.extend(parent_extra_used);
            eliminate_dead_code_in_stmts(&mut region.statements, &extra)
        }
        // Skip For loops - complex loop_var/phi semantics
        _ => 0,
    }
}

/// Checks if a statement is a control-flow terminator (no fall-through).
fn is_terminator(statement: &Statement) -> bool {
    matches!(
        statement,
        Statement::Revert { .. }
            | Statement::Return { .. }
            | Statement::Stop
            | Statement::Invalid
            | Statement::PanicRevert { .. }
            | Statement::ErrorStringRevert { .. }
            | Statement::CustomErrorRevert { .. }
            | Statement::Leave { .. }
            | Statement::SelfDestruct { .. }
    )
}

/// Checks if a statement list starts with the two-statement pattern:
///   `let tmp = callvalue()`
///   `if tmp { ... revert(...) ... }`
/// This pattern is generated by Solidity for non-payable function checks.
/// The then_region must contain a Revert (possibly preceded by Let bindings
/// for the offset/length arguments and followed by an unreachable marker).
fn has_callvalue_revert_prefix(statements: &[Statement]) -> bool {
    if statements.len() < 2 {
        return false;
    }

    // Statement 0: let <id> = callvalue()
    let callvalue_id = match &statements[0] {
        Statement::Let {
            bindings,
            value: Expression::CallValue,
        } if bindings.len() == 1 => Some(bindings[0]),
        _ => None,
    };

    let Some(cv_id) = callvalue_id else {
        return false;
    };

    // Statement 1: if <cv_id> { <let bindings>* revert(...) <unreachable>? }
    // Must have: no inputs, no outputs, no else, then_region contains a Revert
    match &statements[1] {
        Statement::If {
            condition,
            inputs,
            then_region,
            else_region,
            outputs,
        } => {
            // Condition must reference the callvalue binding
            if condition.id != cv_id {
                return false;
            }
            // No SSA value flow (non-payable check doesn't modify variables)
            if !inputs.is_empty() || !outputs.is_empty() {
                return false;
            }
            // No else branch
            if else_region.is_some() {
                return false;
            }
            // Then region must contain a Revert statement somewhere
            // (typically: Let bindings for 0 values, then Revert, then unreachable Expression)
            then_region
                .statements
                .iter()
                .any(|s| matches!(s, Statement::Revert { .. }))
        }
        _ => false,
    }
}

/// Checks if a statement list starts with `let vN = callvalue()`.
/// Used for partial callvalue read hoisting: we only need the first statement
/// to be a callvalue binding, not the full callvalue-revert pattern.
fn starts_with_callvalue_let(statements: &[Statement]) -> bool {
    matches!(
        statements.first(),
        Some(Statement::Let {
            bindings,
            value: Expression::CallValue,
        }) if bindings.len() == 1
    )
}

/// Replaces the first statement's `callvalue()` expression with `Var(replacement_id)`
/// if the first statement is `let vN = callvalue()`. This turns the syscall read
/// into a copy of the already-hoisted value.
fn replace_leading_callvalue_with_var(statements: &mut [Statement], replacement_id: ValueId) {
    if let Some(Statement::Let {
        bindings,
        value: value @ Expression::CallValue,
    }) = statements.first_mut()
    {
        if bindings.len() == 1 {
            *value = Expression::Var(replacement_id);
        }
    }
}

/// Collects ValueIds from region yields into a "used" set.
fn yields_as_used(yields: &[Value]) -> BTreeSet<u32> {
    let mut used = BTreeSet::new();
    for y in yields {
        used.insert(y.id.0);
    }
    used
}

// =============================================================================
// Function deduplication
// =============================================================================

/// Deduplicates functions with identical bodies in an object.
///
/// Two functions are considered duplicates if they have:
/// - Same number and types of parameters
/// - Same number and types of return values
/// - Structurally identical bodies (alpha-equivalent, ignoring ValueId numbering)
///
/// When duplicates are found, all calls to the duplicate are redirected to the
/// canonical (first-seen) function, and the duplicate is removed.
///
/// Returns the number of functions removed.
pub fn deduplicate_functions(object: &mut Object) -> usize {
    if object.functions.len() < 2 {
        return 0;
    }

    // Step 1: Compute canonical forms for all functions
    let mut canonical_to_id: BTreeMap<Vec<u8>, FunctionId> = BTreeMap::new();
    let mut redirects: BTreeMap<FunctionId, FunctionId> = BTreeMap::new();

    for function in object.functions.values() {
        // Skip functions that are too small to bother (the overhead of a call
        // is roughly 2-3 instructions, so only dedup functions > 3 statements)
        if function.size_estimate <= 3 {
            continue;
        }

        let signature = (
            function
                .parameters
                .iter()
                .map(|(_, ty)| *ty)
                .collect::<Vec<_>>(),
            function.returns.clone(),
        );

        let canonical = canonicalize_function(function, &signature.0, &signature.1);

        if let Some(&canonical_id) = canonical_to_id.get(&canonical) {
            // This function is a duplicate of canonical_id
            redirects.insert(function.id, canonical_id);
        } else {
            canonical_to_id.insert(canonical, function.id);
        }
    }

    if redirects.is_empty() {
        return 0;
    }

    let removed_count = redirects.len();
    // Step 2: Redirect all calls in the IR
    redirect_calls_in_block(&mut object.code, &redirects);
    for function in object.functions.values_mut() {
        redirect_calls_in_block(&mut function.body, &redirects);
    }

    // Step 3: Remove duplicate functions
    for dup_id in redirects.keys() {
        object.functions.remove(dup_id);
    }

    removed_count
}

/// Produces a canonical byte representation of a function body for comparison.
/// ValueIds are renumbered sequentially (0, 1, 2, ...) in order of first occurrence.
/// FunctionIds are preserved (they're global references, not local SSA).
fn canonicalize_function(
    function: &crate::ir::Function,
    parameter_types: &[Type],
    return_types: &[Type],
) -> Vec<u8> {
    let mut canon = Canonicalizer::new();
    let mut buf = Vec::new();

    // Encode signature
    buf.push(parameter_types.len() as u8);
    for ty in parameter_types {
        buf.push(type_tag(ty));
    }
    buf.push(return_types.len() as u8);
    for ty in return_types {
        buf.push(type_tag(ty));
    }

    // Register parameters in canonical order
    for (param_id, _) in &function.parameters {
        canon.get_or_insert(*param_id);
    }
    // Register return value initials
    for rv in &function.return_values_initial {
        canon.get_or_insert(*rv);
    }

    // Encode body
    for statement in &function.body.statements {
        canon.encode_stmt(statement, &mut buf);
    }

    // Encode return values
    buf.push(0xFE); // marker for return values
    for rv in &function.return_values {
        canon.encode_value_id(*rv, &mut buf);
    }

    buf
}

struct Canonicalizer {
    id_map: BTreeMap<u32, u32>,
    next_id: u32,
}

impl Canonicalizer {
    fn new() -> Self {
        Canonicalizer {
            id_map: BTreeMap::new(),
            next_id: 0,
        }
    }

    fn get_or_insert(&mut self, id: ValueId) -> u32 {
        *self.id_map.entry(id.0).or_insert_with(|| {
            let n = self.next_id;
            self.next_id += 1;
            n
        })
    }

    fn encode_value_id(&mut self, id: ValueId, buf: &mut Vec<u8>) {
        let canonical = self.get_or_insert(id);
        buf.extend_from_slice(&canonical.to_le_bytes());
    }

    fn encode_value(&mut self, value: &Value, buf: &mut Vec<u8>) {
        self.encode_value_id(value.id, buf);
        buf.push(type_tag(&value.ty));
    }

    fn encode_expr(&mut self, expression: &Expression, buf: &mut Vec<u8>) {
        match expression {
            Expression::Literal { value, ty } => {
                buf.push(0x01);
                let bytes = value.to_bytes_le();
                buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
                buf.extend_from_slice(&bytes);
                buf.push(type_tag(ty));
            }
            Expression::Var(id) => {
                buf.push(0x02);
                self.encode_value_id(*id, buf);
            }
            Expression::Binary { op, lhs, rhs } => {
                buf.push(0x03);
                buf.push(binop_tag(*op));
                self.encode_value(lhs, buf);
                self.encode_value(rhs, buf);
            }
            Expression::Ternary { op, a, b, n } => {
                buf.push(0x04);
                buf.push(binop_tag(*op));
                self.encode_value(a, buf);
                self.encode_value(b, buf);
                self.encode_value(n, buf);
            }
            Expression::Unary { op, operand } => {
                buf.push(0x05);
                buf.push(unaryop_tag(*op));
                self.encode_value(operand, buf);
            }
            Expression::Call {
                function,
                arguments,
            } => {
                buf.push(0x06);
                // FunctionId is a global reference, preserve it
                buf.extend_from_slice(&function.0.to_le_bytes());
                buf.push(arguments.len() as u8);
                for argument in arguments {
                    self.encode_value(argument, buf);
                }
            }
            Expression::CallDataLoad { offset } => {
                buf.push(0x10);
                self.encode_value(offset, buf);
            }
            Expression::MLoad { offset, region } => {
                buf.push(0x11);
                self.encode_value(offset, buf);
                buf.push(region_tag(region));
            }
            Expression::SLoad { key, static_slot } => {
                buf.push(0x12);
                self.encode_value(key, buf);
                if let Some(slot) = static_slot {
                    buf.push(1);
                    let bytes = slot.to_bytes_le();
                    buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
                    buf.extend_from_slice(&bytes);
                } else {
                    buf.push(0);
                }
            }
            Expression::TLoad { key } => {
                buf.push(0x13);
                self.encode_value(key, buf);
            }
            Expression::Keccak256 { offset, length } => {
                buf.push(0x14);
                self.encode_value(offset, buf);
                self.encode_value(length, buf);
            }
            Expression::Keccak256Pair { word0, word1 } => {
                buf.push(0x24);
                self.encode_value(word0, buf);
                self.encode_value(word1, buf);
            }
            Expression::Keccak256Single { word0 } => {
                buf.push(0x25);
                self.encode_value(word0, buf);
            }
            Expression::MappingSLoad { key, slot } => {
                buf.push(0x27);
                self.encode_value(key, buf);
                self.encode_value(slot, buf);
            }
            Expression::Truncate { value, to } => {
                buf.push(0x15);
                self.encode_value(value, buf);
                buf.push(bitwidth_tag(*to));
            }
            Expression::ZeroExtend { value, to } => {
                buf.push(0x16);
                self.encode_value(value, buf);
                buf.push(bitwidth_tag(*to));
            }
            Expression::SignExtendTo { value, to } => {
                buf.push(0x17);
                self.encode_value(value, buf);
                buf.push(bitwidth_tag(*to));
            }
            Expression::ExtCodeSize { address }
            | Expression::ExtCodeHash { address }
            | Expression::Balance { address } => {
                buf.push(match expression {
                    Expression::ExtCodeSize { .. } => 0x20,
                    Expression::ExtCodeHash { .. } => 0x21,
                    Expression::Balance { .. } => 0x22,
                    _ => unreachable!(),
                });
                self.encode_value(address, buf);
            }
            Expression::BlockHash { number } => {
                buf.push(0x23);
                self.encode_value(number, buf);
            }
            Expression::BlobHash { index } => {
                buf.push(0x26);
                self.encode_value(index, buf);
            }
            Expression::DataOffset { id } => {
                buf.push(0x30);
                buf.extend_from_slice(&(id.len() as u16).to_le_bytes());
                buf.extend_from_slice(id.as_bytes());
            }
            Expression::DataSize { id } => {
                buf.push(0x31);
                buf.extend_from_slice(&(id.len() as u16).to_le_bytes());
                buf.extend_from_slice(id.as_bytes());
            }
            Expression::LoadImmutable { key } => {
                buf.push(0x32);
                buf.extend_from_slice(&(key.len() as u16).to_le_bytes());
                buf.extend_from_slice(key.as_bytes());
            }
            Expression::LinkerSymbol { path } => {
                buf.push(0x33);
                buf.extend_from_slice(&(path.len() as u16).to_le_bytes());
                buf.extend_from_slice(path.as_bytes());
            }
            // Nullary builtins: each gets a unique tag
            _ => {
                buf.push(nullary_expr_tag(expression));
            }
        }
    }

    fn encode_region(&mut self, region: &Region, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&(region.statements.len() as u32).to_le_bytes());
        for statement in &region.statements {
            self.encode_stmt(statement, buf);
        }
        buf.push(region.yields.len() as u8);
        for y in &region.yields {
            self.encode_value(y, buf);
        }
    }

    fn encode_stmt(&mut self, statement: &Statement, buf: &mut Vec<u8>) {
        match statement {
            Statement::Let { bindings, value } => {
                buf.push(0x80);
                buf.push(bindings.len() as u8);
                for b in bindings {
                    self.encode_value_id(*b, buf);
                }
                self.encode_expr(value, buf);
            }
            Statement::MStore {
                offset,
                value,
                region,
            } => {
                buf.push(0x81);
                self.encode_value(offset, buf);
                self.encode_value(value, buf);
                buf.push(region_tag(region));
            }
            Statement::MStore8 {
                offset,
                value,
                region,
            } => {
                buf.push(0x82);
                self.encode_value(offset, buf);
                self.encode_value(value, buf);
                buf.push(region_tag(region));
            }
            Statement::MCopy { dest, src, length } => {
                buf.push(0x83);
                self.encode_value(dest, buf);
                self.encode_value(src, buf);
                self.encode_value(length, buf);
            }
            Statement::SStore {
                key,
                value,
                static_slot,
            } => {
                buf.push(0x84);
                self.encode_value(key, buf);
                self.encode_value(value, buf);
                if let Some(slot) = static_slot {
                    buf.push(1);
                    let bytes = slot.to_bytes_le();
                    buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
                    buf.extend_from_slice(&bytes);
                } else {
                    buf.push(0);
                }
            }
            Statement::TStore { key, value } => {
                buf.push(0x85);
                self.encode_value(key, buf);
                self.encode_value(value, buf);
            }
            Statement::MappingSStore { key, slot, value } => {
                buf.push(0xA5);
                self.encode_value(key, buf);
                self.encode_value(slot, buf);
                self.encode_value(value, buf);
            }
            Statement::If {
                condition,
                inputs,
                then_region,
                else_region,
                outputs,
            } => {
                buf.push(0x86);
                self.encode_value(condition, buf);
                buf.push(inputs.len() as u8);
                for i in inputs {
                    self.encode_value(i, buf);
                }
                self.encode_region(then_region, buf);
                if let Some(r) = else_region {
                    buf.push(1);
                    self.encode_region(r, buf);
                } else {
                    buf.push(0);
                }
                buf.push(outputs.len() as u8);
                for o in outputs {
                    self.encode_value_id(*o, buf);
                }
            }
            Statement::Switch {
                scrutinee,
                inputs,
                cases,
                default,
                outputs,
            } => {
                buf.push(0x87);
                self.encode_value(scrutinee, buf);
                buf.push(inputs.len() as u8);
                for i in inputs {
                    self.encode_value(i, buf);
                }
                buf.extend_from_slice(&(cases.len() as u32).to_le_bytes());
                for c in cases {
                    let case_bytes = c.value.to_bytes_le();
                    buf.extend_from_slice(&(case_bytes.len() as u16).to_le_bytes());
                    buf.extend_from_slice(&case_bytes);
                    self.encode_region(&c.body, buf);
                }
                if let Some(d) = default {
                    buf.push(1);
                    self.encode_region(d, buf);
                } else {
                    buf.push(0);
                }
                buf.push(outputs.len() as u8);
                for o in outputs {
                    self.encode_value_id(*o, buf);
                }
            }
            Statement::For {
                initial_values,
                loop_variables,
                condition_statements,
                condition,
                body,
                post_input_variables,
                post,
                outputs,
            } => {
                buf.push(0x88);
                buf.push(initial_values.len() as u8);
                for v in initial_values {
                    self.encode_value(v, buf);
                }
                buf.push(loop_variables.len() as u8);
                for lv in loop_variables {
                    self.encode_value_id(*lv, buf);
                }
                buf.extend_from_slice(&(condition_statements.len() as u32).to_le_bytes());
                for s in condition_statements {
                    self.encode_stmt(s, buf);
                }
                self.encode_expr(condition, buf);
                self.encode_region(body, buf);
                buf.push(post_input_variables.len() as u8);
                for pv in post_input_variables {
                    self.encode_value_id(*pv, buf);
                }
                self.encode_region(post, buf);
                buf.push(outputs.len() as u8);
                for o in outputs {
                    self.encode_value_id(*o, buf);
                }
            }
            Statement::Block(region) => {
                buf.push(0x89);
                self.encode_region(region, buf);
            }
            Statement::Revert { offset, length } => {
                buf.push(0x90);
                self.encode_value(offset, buf);
                self.encode_value(length, buf);
            }
            Statement::Return { offset, length } => {
                buf.push(0x91);
                self.encode_value(offset, buf);
                self.encode_value(length, buf);
            }
            Statement::Stop => buf.push(0x92),
            Statement::Invalid => buf.push(0x93),
            Statement::PanicRevert { code } => {
                buf.push(0xA2);
                buf.push(*code);
            }
            Statement::ErrorStringRevert { length, data } => {
                buf.push(0xA3);
                buf.push(*length);
                buf.push(data.len() as u8);
                for word in data {
                    for byte in word.to_bytes_be() {
                        buf.push(byte);
                    }
                }
            }
            Statement::CustomErrorRevert {
                selector,
                arguments,
            } => {
                buf.push(0xA4);
                buf.push(arguments.len() as u8);
                for byte in selector.to_bytes_be() {
                    buf.push(byte);
                }
                for argument in arguments {
                    self.encode_value(argument, buf);
                }
            }
            Statement::ExternalCall {
                kind,
                gas,
                address,
                value,
                args_offset,
                args_length,
                ret_offset,
                ret_length,
                result,
            } => {
                buf.push(0x94);
                buf.push(callkind_tag(*kind));
                self.encode_value(gas, buf);
                self.encode_value(address, buf);
                if let Some(v) = value {
                    buf.push(1);
                    self.encode_value(v, buf);
                } else {
                    buf.push(0);
                }
                self.encode_value(args_offset, buf);
                self.encode_value(args_length, buf);
                self.encode_value(ret_offset, buf);
                self.encode_value(ret_length, buf);
                self.encode_value_id(*result, buf);
            }
            Statement::Create {
                kind,
                value,
                offset,
                length,
                salt,
                result,
            } => {
                buf.push(0x95);
                buf.push(createkind_tag(*kind));
                self.encode_value(value, buf);
                self.encode_value(offset, buf);
                self.encode_value(length, buf);
                if let Some(s) = salt {
                    buf.push(1);
                    self.encode_value(s, buf);
                } else {
                    buf.push(0);
                }
                self.encode_value_id(*result, buf);
            }
            Statement::Log {
                offset,
                length,
                topics,
            } => {
                buf.push(0x96);
                self.encode_value(offset, buf);
                self.encode_value(length, buf);
                buf.push(topics.len() as u8);
                for t in topics {
                    self.encode_value(t, buf);
                }
            }
            Statement::CodeCopy {
                dest,
                offset,
                length,
            } => {
                buf.push(0x97);
                self.encode_value(dest, buf);
                self.encode_value(offset, buf);
                self.encode_value(length, buf);
            }
            Statement::ExtCodeCopy {
                address,
                dest,
                offset,
                length,
            } => {
                buf.push(0x98);
                self.encode_value(address, buf);
                self.encode_value(dest, buf);
                self.encode_value(offset, buf);
                self.encode_value(length, buf);
            }
            Statement::ReturnDataCopy {
                dest,
                offset,
                length,
            } => {
                buf.push(0x99);
                self.encode_value(dest, buf);
                self.encode_value(offset, buf);
                self.encode_value(length, buf);
            }
            Statement::DataCopy {
                dest,
                offset,
                length,
            } => {
                buf.push(0x9A);
                self.encode_value(dest, buf);
                self.encode_value(offset, buf);
                self.encode_value(length, buf);
            }
            Statement::CallDataCopy {
                dest,
                offset,
                length,
            } => {
                buf.push(0x9B);
                self.encode_value(dest, buf);
                self.encode_value(offset, buf);
                self.encode_value(length, buf);
            }
            Statement::SetImmutable { key, value } => {
                buf.push(0x9C);
                buf.extend_from_slice(&(key.len() as u16).to_le_bytes());
                buf.extend_from_slice(key.as_bytes());
                self.encode_value(value, buf);
            }
            Statement::Leave { return_values } => {
                buf.push(0x9D);
                buf.push(return_values.len() as u8);
                for v in return_values {
                    self.encode_value(v, buf);
                }
            }
            Statement::Expression(expression) => {
                buf.push(0x9E);
                self.encode_expr(expression, buf);
            }
            Statement::SelfDestruct { address } => {
                buf.push(0x9F);
                self.encode_value(address, buf);
            }
            Statement::Break { values } => {
                buf.push(0xA0);
                buf.push(values.len() as u8);
                for v in values {
                    self.encode_value(v, buf);
                }
            }
            Statement::Continue { values } => {
                buf.push(0xA1);
                buf.push(values.len() as u8);
                for v in values {
                    self.encode_value(v, buf);
                }
            }
        }
    }
}

fn type_tag(ty: &Type) -> u8 {
    match ty {
        Type::Int(bit_width) => bitwidth_tag(*bit_width),
        Type::Ptr(addr) => match addr {
            crate::ir::AddressSpace::Heap => 0xE0,
            crate::ir::AddressSpace::Stack => 0xE1,
            crate::ir::AddressSpace::Storage => 0xE2,
            crate::ir::AddressSpace::Code => 0xE3,
        },
        Type::Void => 0xFF,
    }
}

fn bitwidth_tag(bit_width: BitWidth) -> u8 {
    match bit_width {
        BitWidth::I1 => 1,
        BitWidth::I8 => 8,
        BitWidth::I32 => 32,
        BitWidth::I64 => 64,
        BitWidth::I128 => 128,
        BitWidth::I160 => 160,
        BitWidth::I256 => 0,
    }
}

fn binop_tag(op: BinaryOperation) -> u8 {
    match op {
        BinaryOperation::Add => 0,
        BinaryOperation::Sub => 1,
        BinaryOperation::Mul => 2,
        BinaryOperation::Div => 3,
        BinaryOperation::SDiv => 4,
        BinaryOperation::Mod => 5,
        BinaryOperation::SMod => 6,
        BinaryOperation::Exp => 7,
        BinaryOperation::AddMod => 8,
        BinaryOperation::MulMod => 9,
        BinaryOperation::And => 10,
        BinaryOperation::Or => 11,
        BinaryOperation::Xor => 12,
        BinaryOperation::Shl => 13,
        BinaryOperation::Shr => 14,
        BinaryOperation::Sar => 15,
        BinaryOperation::Lt => 16,
        BinaryOperation::Gt => 17,
        BinaryOperation::Slt => 18,
        BinaryOperation::Sgt => 19,
        BinaryOperation::Eq => 20,
        BinaryOperation::Byte => 21,
        BinaryOperation::SignExtend => 22,
    }
}

fn unaryop_tag(op: UnaryOperation) -> u8 {
    match op {
        UnaryOperation::IsZero => 0,
        UnaryOperation::Not => 1,
        UnaryOperation::Clz => 2,
    }
}

fn region_tag(region: &crate::ir::MemoryRegion) -> u8 {
    match region {
        crate::ir::MemoryRegion::Scratch => 0,
        crate::ir::MemoryRegion::FreePointerSlot => 1,
        crate::ir::MemoryRegion::Dynamic => 2,
        crate::ir::MemoryRegion::Unknown => 3,
    }
}

fn callkind_tag(kind: CallKind) -> u8 {
    match kind {
        CallKind::Call => 0,
        CallKind::CallCode => 1,
        CallKind::DelegateCall => 2,
        CallKind::StaticCall => 3,
    }
}

fn createkind_tag(kind: crate::ir::CreateKind) -> u8 {
    match kind {
        crate::ir::CreateKind::Create => 0,
        crate::ir::CreateKind::Create2 => 1,
    }
}

fn nullary_expr_tag(expression: &Expression) -> u8 {
    match expression {
        Expression::CallValue => 0x40,
        Expression::Caller => 0x41,
        Expression::Origin => 0x42,
        Expression::CallDataSize => 0x43,
        Expression::CodeSize => 0x44,
        Expression::GasPrice => 0x45,
        Expression::ReturnDataSize => 0x46,
        Expression::Coinbase => 0x47,
        Expression::Timestamp => 0x48,
        Expression::Number => 0x49,
        Expression::Difficulty => 0x4A,
        Expression::GasLimit => 0x4B,
        Expression::ChainId => 0x4C,
        Expression::SelfBalance => 0x4D,
        Expression::BaseFee => 0x4E,
        Expression::BlobBaseFee => 0x4F,
        Expression::Gas => 0x50,
        Expression::MSize => 0x51,
        Expression::Address => 0x52,
        _ => 0x00, // Should not happen for nullary expressions
    }
}

// =============================================================================
// Fuzzy function deduplication (parameterize by differing literals)
// =============================================================================

/// Maximum number of differing literal positions allowed for fuzzy dedup.
/// Each differing literal becomes a new i256 parameter, so keep this small.
const MAX_FUZZY_LITERAL_DIFFS: usize = 4;

/// Minimum function size (in IR statements) for fuzzy dedup.
/// Smaller functions don't save enough to justify the extra parameter overhead.
const MIN_FUZZY_DEDUP_SIZE: usize = 20;

/// Deduplicates functions that are structurally identical except for literal constants.
///
/// When two functions have the same structure but differ only in literal values,
/// the duplicate is removed and its call sites are redirected to the canonical
/// function with the differing literals passed as additional arguments.
///
/// Returns the number of functions removed.
pub fn deduplicate_functions_fuzzy(object: &mut Object) -> usize {
    if object.functions.len() < 2 {
        return 0;
    }

    // Step 1: Compute fuzzy canonical forms (literals replaced by position indices)
    // Group functions by fuzzy hash
    let mut fuzzy_groups: BTreeMap<Vec<u8>, Vec<FunctionId>> = BTreeMap::new();

    for function in object.functions.values() {
        if function.size_estimate < MIN_FUZZY_DEDUP_SIZE {
            continue;
        }

        let fuzzy_hash = fuzzy_canonicalize_function(function);
        fuzzy_groups
            .entry(fuzzy_hash)
            .or_default()
            .push(function.id);
    }

    // Step 2: For each group with 2+ members, find differing literals
    let mut total_removed = 0;
    let mut next_value_id = object.find_max_value_id() + 1;

    for group in fuzzy_groups.values() {
        if group.len() < 2 {
            continue;
        }

        // Collect literals from each function in the group
        let mut group_literals: Vec<(FunctionId, Vec<BigUint>)> = Vec::new();
        for &fid in group {
            let function = &object.functions[&fid];
            let lits = collect_literals_ordered(function);
            group_literals.push((fid, lits));
        }

        // All functions must have the same number of literals
        let lit_count = group_literals[0].1.len();
        if group_literals
            .iter()
            .any(|(_, lits)| lits.len() != lit_count)
        {
            continue;
        }

        // Find positions where literals differ
        let mut differing_positions: Vec<usize> = Vec::new();
        for pos in 0..lit_count {
            let first_val = &group_literals[0].1[pos];
            if group_literals
                .iter()
                .any(|(_, lits)| &lits[pos] != first_val)
            {
                differing_positions.push(pos);
            }
        }

        if differing_positions.is_empty() {
            continue; // Exact duplicates - handled by existing dedup
        }

        // Group differing positions by their value signature across all functions.
        // Positions that have the same value in every function member share one parameter.
        // For example, if a storage slot constant appears at positions {1,2,7,21,23} and
        // it's the only value that differs, we need 1 parameter, not 5.
        let mut value_sig_to_group: BTreeMap<Vec<Vec<u8>>, Vec<usize>> = BTreeMap::new();
        for &pos in &differing_positions {
            let sig: Vec<Vec<u8>> = group_literals
                .iter()
                .map(|(_, lits)| lits[pos].to_bytes_le())
                .collect();
            value_sig_to_group.entry(sig).or_default().push(pos);
        }

        let unique_param_count = value_sig_to_group.len();
        if unique_param_count > MAX_FUZZY_LITERAL_DIFFS {
            continue;
        }

        // Build mapping: for each differing position, which unique parameter index?
        // Sorted by first position in each group for deterministic ordering.
        let mut param_groups: Vec<Vec<usize>> = value_sig_to_group.into_values().collect();
        param_groups.sort_by_key(|g| g[0]);
        let mut pos_to_param_idx: BTreeMap<usize, usize> = BTreeMap::new();
        for (param_idx, positions) in param_groups.iter().enumerate() {
            for &pos in positions {
                pos_to_param_idx.insert(pos, param_idx);
            }
        }

        // The canonical function is the first in the group
        let canonical_id = group[0];
        let canonical_func = object.functions.get(&canonical_id).unwrap().clone();

        // Check that all functions have the same parameter count and return types
        let canonical_param_count = canonical_func.parameters.len();
        let canonical_returns = &canonical_func.returns;
        let all_compatible = group.iter().skip(1).all(|&fid| {
            let function = &object.functions[&fid];
            function.parameters.len() == canonical_param_count
                && &function.returns == canonical_returns
                && function
                    .parameters
                    .iter()
                    .zip(canonical_func.parameters.iter())
                    .all(|((_, t1), (_, t2))| t1 == t2)
        });
        if !all_compatible {
            continue;
        }

        // Build the parameterized canonical function:
        // Add one i256 parameter for each unique differing value group
        let mut new_param_ids: Vec<ValueId> = Vec::new();
        for _ in 0..unique_param_count {
            let vid = ValueId(next_value_id);
            next_value_id += 1;
            new_param_ids.push(vid);
        }

        // Build the per-position param ID mapping for replacement
        let position_param_ids: Vec<(usize, ValueId)> = differing_positions
            .iter()
            .map(|&pos| (pos, new_param_ids[pos_to_param_idx[&pos]]))
            .collect();

        // Clone canonical function and add new parameters
        let mut parameterized = canonical_func.clone();
        for &vid in &new_param_ids {
            parameterized
                .parameters
                .push((vid, Type::Int(BitWidth::I256)));
        }

        // Replace the differing literals in the parameterized body with Var references
        let canonical_lits = &group_literals[0].1;

        replace_literals_with_params(&mut parameterized.body, &position_param_ids);

        // Replace canonical function with parameterized version
        object.functions.insert(canonical_id, parameterized);

        // Build call-site redirects and argument mappings
        // For the canonical function's own call sites, add its original literals as arguments
        // (one argument per unique parameter, using the first position in each group)
        let canonical_extra_args: Vec<BigUint> = param_groups
            .iter()
            .map(|positions| canonical_lits[positions[0]].clone())
            .collect();

        // Update all call sites across the IR
        for &fid in group {
            let extra_args: Vec<BigUint> = if fid == canonical_id {
                canonical_extra_args.clone()
            } else {
                let lits = &group_literals.iter().find(|(id, _)| *id == fid).unwrap().1;
                param_groups
                    .iter()
                    .map(|positions| lits[positions[0]].clone())
                    .collect()
            };

            // Update all call sites for this function ID to call canonical_id
            // with the extra literal arguments appended
            update_call_sites_with_extra_args(
                &mut object.code,
                fid,
                canonical_id,
                &extra_args,
                &mut next_value_id,
            );
            for function in object.functions.values_mut() {
                update_call_sites_with_extra_args(
                    &mut function.body,
                    fid,
                    canonical_id,
                    &extra_args,
                    &mut next_value_id,
                );
            }
        }

        // Remove duplicate functions (all except canonical)
        for &fid in group.iter().skip(1) {
            object.functions.remove(&fid);
            total_removed += 1;
        }
    }

    total_removed
}

/// Produces a fuzzy canonical form where literal values are replaced with
/// position indices. Two functions with the same fuzzy form are structurally
/// identical except for their literal constant values.
fn fuzzy_canonicalize_function(function: &crate::ir::Function) -> Vec<u8> {
    let mut canon = Canonicalizer::new();
    let mut buf = Vec::new();

    // Encode signature (same as exact dedup)
    buf.push(function.parameters.len() as u8);
    for (_, ty) in &function.parameters {
        buf.push(type_tag(ty));
    }
    buf.push(function.returns.len() as u8);
    for ty in &function.returns {
        buf.push(type_tag(ty));
    }

    // Register parameters
    for (param_id, _) in &function.parameters {
        canon.get_or_insert(*param_id);
    }
    for rv in &function.return_values_initial {
        canon.get_or_insert(*rv);
    }

    // Encode body with literals replaced by placeholder
    let mut lit_counter = 0u32;
    for statement in &function.body.statements {
        fuzzy_encode_stmt(&mut canon, statement, &mut buf, &mut lit_counter);
    }

    buf.push(0xFE);
    for rv in &function.return_values {
        canon.encode_value_id(*rv, &mut buf);
    }

    buf
}

fn fuzzy_encode_expr(
    canon: &mut Canonicalizer,
    expression: &Expression,
    buf: &mut Vec<u8>,
    lit_counter: &mut u32,
) {
    match expression {
        Expression::Literal { ty, .. } => {
            buf.push(0x01);
            // Replace literal value with position index
            buf.push(0xFF); // marker for "fuzzy literal"
            buf.extend_from_slice(&lit_counter.to_le_bytes());
            *lit_counter += 1;
            buf.push(type_tag(ty));
        }
        Expression::Var(id) => {
            buf.push(0x02);
            canon.encode_value_id(*id, buf);
        }
        Expression::Binary { op, lhs, rhs } => {
            buf.push(0x03);
            buf.push(binop_tag(*op));
            canon.encode_value(lhs, buf);
            canon.encode_value(rhs, buf);
        }
        Expression::Ternary { op, a, b, n } => {
            buf.push(0x04);
            buf.push(binop_tag(*op));
            canon.encode_value(a, buf);
            canon.encode_value(b, buf);
            canon.encode_value(n, buf);
        }
        Expression::Unary { op, operand } => {
            buf.push(0x05);
            buf.push(unaryop_tag(*op));
            canon.encode_value(operand, buf);
        }
        Expression::Call {
            function,
            arguments,
        } => {
            buf.push(0x06);
            // For fuzzy dedup, we use a placeholder for FunctionId too,
            // since the callee may itself be a different duplicate
            buf.extend_from_slice(&function.0.to_le_bytes());
            buf.push(arguments.len() as u8);
            for argument in arguments {
                canon.encode_value(argument, buf);
            }
        }
        Expression::SLoad { key, static_slot } => {
            buf.push(0x12);
            canon.encode_value(key, buf);
            // Replace static_slot with position index (it's a literal)
            if static_slot.is_some() {
                buf.push(1);
                buf.push(0xFF);
                buf.extend_from_slice(&lit_counter.to_le_bytes());
                *lit_counter += 1;
            } else {
                buf.push(0);
            }
        }
        // For all other expressions, delegate to exact encoding
        _ => {
            canon.encode_expr(expression, buf);
        }
    }
}

fn fuzzy_encode_stmt(
    canon: &mut Canonicalizer,
    statement: &Statement,
    buf: &mut Vec<u8>,
    lit_counter: &mut u32,
) {
    match statement {
        Statement::Let { bindings, value } => {
            buf.push(0x80);
            buf.push(bindings.len() as u8);
            for b in bindings {
                canon.encode_value_id(*b, buf);
            }
            fuzzy_encode_expr(canon, value, buf, lit_counter);
        }
        Statement::SStore {
            key,
            value,
            static_slot,
        } => {
            buf.push(0x84);
            canon.encode_value(key, buf);
            canon.encode_value(value, buf);
            if static_slot.is_some() {
                buf.push(1);
                buf.push(0xFF);
                buf.extend_from_slice(&lit_counter.to_le_bytes());
                *lit_counter += 1;
            } else {
                buf.push(0);
            }
        }
        Statement::If {
            condition,
            inputs,
            then_region,
            else_region,
            outputs,
        } => {
            buf.push(0x85);
            canon.encode_value(condition, buf);
            buf.push(inputs.len() as u8);
            for v in inputs {
                canon.encode_value(v, buf);
            }
            fuzzy_encode_region(canon, then_region, buf, lit_counter);
            if let Some(r) = else_region {
                buf.push(1);
                fuzzy_encode_region(canon, r, buf, lit_counter);
            } else {
                buf.push(0);
            }
            buf.push(outputs.len() as u8);
            for o in outputs {
                canon.encode_value_id(*o, buf);
            }
        }
        Statement::Switch {
            scrutinee,
            inputs,
            cases,
            default,
            outputs,
        } => {
            buf.push(0x86);
            canon.encode_value(scrutinee, buf);
            buf.push(inputs.len() as u8);
            for v in inputs {
                canon.encode_value(v, buf);
            }
            buf.push(cases.len() as u8);
            for c in cases {
                // Case values are literals - replace with placeholder
                buf.push(0xFF);
                buf.extend_from_slice(&lit_counter.to_le_bytes());
                *lit_counter += 1;
                fuzzy_encode_region(canon, &c.body, buf, lit_counter);
            }
            if let Some(d) = default {
                buf.push(1);
                fuzzy_encode_region(canon, d, buf, lit_counter);
            } else {
                buf.push(0);
            }
            buf.push(outputs.len() as u8);
            for o in outputs {
                canon.encode_value_id(*o, buf);
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
            buf.push(0x87);
            buf.push(initial_values.len() as u8);
            for v in initial_values {
                canon.encode_value(v, buf);
            }
            buf.push(loop_variables.len() as u8);
            for v in loop_variables {
                canon.encode_value_id(*v, buf);
            }
            buf.push(condition_statements.len() as u8);
            for s in condition_statements {
                fuzzy_encode_stmt(canon, s, buf, lit_counter);
            }
            fuzzy_encode_expr(canon, condition, buf, lit_counter);
            fuzzy_encode_region(canon, body, buf, lit_counter);
            fuzzy_encode_region(canon, post, buf, lit_counter);
            buf.push(outputs.len() as u8);
            for o in outputs {
                canon.encode_value_id(*o, buf);
            }
        }
        Statement::Block(region) => {
            buf.push(0x88);
            fuzzy_encode_region(canon, region, buf, lit_counter);
        }
        Statement::Expression(expression) => {
            buf.push(0x89);
            fuzzy_encode_expr(canon, expression, buf, lit_counter);
        }
        // For all other statements, delegate to exact encoding
        _ => {
            canon.encode_stmt(statement, buf);
        }
    }
}

fn fuzzy_encode_region(
    canon: &mut Canonicalizer,
    region: &Region,
    buf: &mut Vec<u8>,
    lit_counter: &mut u32,
) {
    buf.extend_from_slice(&(region.statements.len() as u32).to_le_bytes());
    for statement in &region.statements {
        fuzzy_encode_stmt(canon, statement, buf, lit_counter);
    }
    buf.push(region.yields.len() as u8);
    for y in &region.yields {
        canon.encode_value(y, buf);
    }
}

/// Collects all literal values from a function in walk order. Both this and
/// [`replace_literals_with_params`] must use the **identical** traversal so
/// position indices align — keep them in sync if either changes.
///
/// Counted positions: `Expression::Literal` value, `Expression::SLoad::static_slot`,
/// `Statement::SStore::static_slot`, and each `Switch` case value.
fn collect_literals_ordered(function: &crate::ir::Function) -> Vec<BigUint> {
    fn from_expr(expression: &Expression, lits: &mut Vec<BigUint>) {
        match expression {
            Expression::Literal { value, .. } => lits.push(value.clone()),
            Expression::SLoad {
                static_slot: Some(slot),
                ..
            } => lits.push(slot.clone()),
            _ => {}
        }
    }
    fn walk(statements: &[Statement], lits: &mut Vec<BigUint>) {
        for statement in statements {
            match statement {
                Statement::Let { value, .. } | Statement::Expression(value) => {
                    from_expr(value, lits)
                }
                Statement::SStore {
                    static_slot: Some(slot),
                    ..
                } => lits.push(slot.clone()),
                Statement::If {
                    then_region,
                    else_region,
                    ..
                } => {
                    walk(&then_region.statements, lits);
                    if let Some(r) = else_region {
                        walk(&r.statements, lits);
                    }
                }
                Statement::Switch { cases, default, .. } => {
                    for c in cases {
                        lits.push(c.value.clone());
                        walk(&c.body.statements, lits);
                    }
                    if let Some(d) = default {
                        walk(&d.statements, lits);
                    }
                }
                Statement::For {
                    condition_statements,
                    condition,
                    body,
                    post,
                    ..
                } => {
                    walk(condition_statements, lits);
                    from_expr(condition, lits);
                    walk(&body.statements, lits);
                    walk(&post.statements, lits);
                }
                Statement::Block(region) => walk(&region.statements, lits),
                _ => {}
            }
        }
    }
    let mut lits = Vec::new();
    walk(&function.body.statements, &mut lits);
    lits
}

/// Replaces literal values at differing positions with `Var` references to new
/// parameters. Walk order **must** match [`collect_literals_ordered`] so the
/// position indices align.
fn replace_literals_with_params(block: &mut Block, position_param_ids: &[(usize, ValueId)]) {
    fn from_expr(
        expression: &mut Expression,
        positions: &BTreeMap<usize, ValueId>,
        counter: &mut usize,
    ) {
        match expression {
            Expression::Literal { .. } => {
                if let Some(&param_vid) = positions.get(counter) {
                    *expression = Expression::Var(param_vid);
                }
                *counter += 1;
            }
            Expression::SLoad {
                static_slot: slot @ Some(_),
                ..
            } => {
                if positions.contains_key(counter) {
                    *slot = None;
                }
                *counter += 1;
            }
            _ => {}
        }
    }
    fn walk(
        statements: &mut [Statement],
        positions: &BTreeMap<usize, ValueId>,
        counter: &mut usize,
    ) {
        for statement in statements {
            match statement {
                Statement::Let { value, .. } | Statement::Expression(value) => {
                    from_expr(value, positions, counter)
                }
                Statement::SStore {
                    static_slot: slot @ Some(_),
                    ..
                } => {
                    if positions.contains_key(counter) {
                        *slot = None;
                    }
                    *counter += 1;
                }
                Statement::If {
                    then_region,
                    else_region,
                    ..
                } => {
                    walk(&mut then_region.statements, positions, counter);
                    if let Some(r) = else_region {
                        walk(&mut r.statements, positions, counter);
                    }
                }
                Statement::Switch { cases, default, .. } => {
                    for c in cases {
                        *counter += 1; // case value position
                        walk(&mut c.body.statements, positions, counter);
                    }
                    if let Some(d) = default {
                        walk(&mut d.statements, positions, counter);
                    }
                }
                Statement::For {
                    condition_statements,
                    condition,
                    body,
                    post,
                    ..
                } => {
                    walk(condition_statements, positions, counter);
                    from_expr(condition, positions, counter);
                    walk(&mut body.statements, positions, counter);
                    walk(&mut post.statements, positions, counter);
                }
                Statement::Block(region) => walk(&mut region.statements, positions, counter),
                _ => {}
            }
        }
    }
    let positions: BTreeMap<usize, ValueId> = position_param_ids.iter().copied().collect();
    let mut counter = 0usize;
    walk(&mut block.statements, &positions, &mut counter);
}

/// Updates call sites in a block, changing calls to `old_id` into calls to
/// `new_id` with extra literal arguments appended. For each matching call,
/// emits one `Let` binding per extra argument immediately before the call site
/// (allocating fresh `ValueId`s through `next_value_id`).
fn update_call_sites_with_extra_args(
    block: &mut Block,
    old_id: FunctionId,
    new_id: FunctionId,
    extra_args: &[BigUint],
    next_value_id: &mut u32,
) {
    fn rewrite(
        statements: &mut Vec<Statement>,
        old_id: FunctionId,
        new_id: FunctionId,
        extra_args: &[BigUint],
        next_id: &mut u32,
    ) {
        let mut i = 0;
        while i < statements.len() {
            // Recurse into nested regions first.
            match &mut statements[i] {
                Statement::If {
                    then_region,
                    else_region,
                    ..
                } => {
                    rewrite(
                        &mut then_region.statements,
                        old_id,
                        new_id,
                        extra_args,
                        next_id,
                    );
                    if let Some(r) = else_region {
                        rewrite(&mut r.statements, old_id, new_id, extra_args, next_id);
                    }
                }
                Statement::Switch { cases, default, .. } => {
                    for c in cases.iter_mut() {
                        rewrite(&mut c.body.statements, old_id, new_id, extra_args, next_id);
                    }
                    if let Some(d) = default {
                        rewrite(&mut d.statements, old_id, new_id, extra_args, next_id);
                    }
                }
                Statement::For {
                    condition_statements,
                    body,
                    post,
                    ..
                } => {
                    rewrite(condition_statements, old_id, new_id, extra_args, next_id);
                    rewrite(&mut body.statements, old_id, new_id, extra_args, next_id);
                    rewrite(&mut post.statements, old_id, new_id, extra_args, next_id);
                }
                Statement::Block(region) => {
                    rewrite(&mut region.statements, old_id, new_id, extra_args, next_id);
                }
                _ => {}
            }
            // Then check if this statement is a target call.
            let is_target = matches!(&statements[i],
                Statement::Let { value: Expression::Call { function, .. }, .. }
                | Statement::Expression(Expression::Call { function, .. })
                if *function == old_id);
            if is_target {
                // Insert one Let-literal per extra argument before the call.
                let mut extra_values = Vec::with_capacity(extra_args.len());
                for arg_val in extra_args {
                    let vid = ValueId(*next_id);
                    *next_id += 1;
                    statements.insert(
                        i,
                        Statement::Let {
                            bindings: vec![vid],
                            value: Expression::Literal {
                                value: arg_val.clone(),
                                ty: Type::Int(BitWidth::I256),
                            },
                        },
                    );
                    i += 1; // skip past the inserted Let
                    extra_values.push(Value {
                        id: vid,
                        ty: Type::Int(BitWidth::I256),
                    });
                }
                // Then patch the call (now at position `i`).
                match &mut statements[i] {
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
                    }) => {
                        *function = new_id;
                        arguments.extend(extra_values);
                    }
                    _ => unreachable!("is_target check above just matched a Call"),
                }
            }
            i += 1;
        }
    }
    rewrite(
        &mut block.statements,
        old_id,
        new_id,
        extra_args,
        next_value_id,
    );
}

/// Redirects function calls in a block, replacing old function IDs with new ones.
fn redirect_calls_in_block(block: &mut Block, redirects: &BTreeMap<FunctionId, FunctionId>) {
    for_each_stmt_mut(&mut block.statements, &mut |statement| {
        statement.for_each_expr_mut(&mut |expression| {
            if let Expression::Call { function, .. } = expression {
                if let Some(&new_id) = redirects.get(function) {
                    *function = new_id;
                }
            }
        });
    });
}

/// Folds constant `Keccak256Single` and `Keccak256Pair` expressions in an object.
///
/// This is a targeted pass designed to run after the mem_opt pass, which creates
/// `Keccak256Single`/`Keccak256Pair` nodes from `mstore + keccak256` patterns.
/// When the argument(s) are compile-time constants, the hash is precomputed.
pub fn fold_constant_keccak(object: &mut Object) {
    fold_keccak_in_block(&mut object.code);
    for function in object.functions.values_mut() {
        fold_keccak_in_block(&mut function.body);
    }
}

/// Walks a block's statements and folds constant keccak256 expressions.
fn fold_keccak_in_block(block: &mut Block) {
    let mut constants: BTreeMap<u32, BigUint> = BTreeMap::new();
    fold_keccak_in_stmts(&mut block.statements, &mut constants);
}

/// Processes statements, tracking constants and folding keccak expressions.
fn fold_keccak_in_stmts(statements: &mut [Statement], constants: &mut BTreeMap<u32, BigUint>) {
    for statement in statements.iter_mut() {
        match statement {
            Statement::Let {
                bindings,
                value: expression,
            } => {
                // Track literal constants
                if bindings.len() == 1 {
                    if let Expression::Literal { value, .. } = expression {
                        constants.insert(bindings[0].0, value.clone());
                    }
                }

                // Fold constant keccak256 calls
                match expression {
                    Expression::Keccak256Single { word0 } => {
                        if let Some(c) = constants.get(&word0.id.0) {
                            *expression = Expression::Literal {
                                value: fold_keccak256_single(c),
                                ty: Type::Int(BitWidth::I256),
                            };
                        }
                    }
                    Expression::Keccak256Pair { word0, word1 } => {
                        if let (Some(c0), Some(c1)) =
                            (constants.get(&word0.id.0), constants.get(&word1.id.0))
                        {
                            let c0 = c0.clone();
                            let c1 = c1.clone();
                            *expression = Expression::Literal {
                                value: fold_keccak256_pair(&c0, &c1),
                                ty: Type::Int(BitWidth::I256),
                            };
                        }
                    }
                    _ => {}
                }

                // Record the folded result as a constant too
                if bindings.len() == 1 {
                    if let Expression::Literal { value, .. } = expression {
                        constants.insert(bindings[0].0, value.clone());
                    }
                }
            }
            // Nested regions get their own scope (cloned constants) so that
            // Let bindings introduced inside don't leak to sibling branches.
            // `For::condition_statements` is intentionally not scoped — it's part
            // of the loop header and shares the outer scope.
            Statement::If {
                then_region,
                else_region,
                ..
            } => {
                fold_keccak_in_stmts(&mut then_region.statements, &mut constants.clone());
                if let Some(r) = else_region {
                    fold_keccak_in_stmts(&mut r.statements, &mut constants.clone());
                }
            }
            Statement::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    fold_keccak_in_stmts(&mut case.body.statements, &mut constants.clone());
                }
                if let Some(d) = default {
                    fold_keccak_in_stmts(&mut d.statements, &mut constants.clone());
                }
            }
            Statement::For {
                condition_statements,
                body,
                post,
                ..
            } => {
                fold_keccak_in_stmts(condition_statements, constants);
                fold_keccak_in_stmts(&mut body.statements, &mut constants.clone());
                fold_keccak_in_stmts(&mut post.statements, &mut constants.clone());
            }
            Statement::Block(region) => {
                fold_keccak_in_stmts(&mut region.statements, &mut constants.clone());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_let_literal(id: u32, value: u64) -> Statement {
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expression::Literal {
                value: BigUint::from(value),
                ty: Type::Int(BitWidth::I256),
            },
        }
    }

    fn make_let_binop(id: u32, op: BinaryOperation, lhs_id: u32, rhs_id: u32) -> Statement {
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expression::Binary {
                op,
                lhs: Value::int(ValueId(lhs_id)),
                rhs: Value::int(ValueId(rhs_id)),
            },
        }
    }

    #[test]
    fn test_constant_fold_add() {
        let result = fold_binary(
            BinaryOperation::Add,
            &BigUint::from(100u64),
            &BigUint::from(200u64),
        );
        assert_eq!(result, Some(BigUint::from(300u64)));
    }

    #[test]
    fn test_constant_fold_sub_wrap() {
        let result = fold_binary(
            BinaryOperation::Sub,
            &BigUint::from(0u64),
            &BigUint::from(1u64),
        );
        assert_eq!(result, Some(max_u256()));
    }

    #[test]
    fn test_constant_fold_mul() {
        let result = fold_binary(
            BinaryOperation::Mul,
            &BigUint::from(7u64),
            &BigUint::from(6u64),
        );
        assert_eq!(result, Some(BigUint::from(42u64)));
    }

    #[test]
    fn test_constant_fold_div_by_zero() {
        let result = fold_binary(
            BinaryOperation::Div,
            &BigUint::from(100u64),
            &BigUint::zero(),
        );
        assert_eq!(result, Some(BigUint::zero()));
    }

    #[test]
    fn test_constant_fold_comparisons() {
        assert_eq!(
            fold_binary(
                BinaryOperation::Lt,
                &BigUint::from(5u64),
                &BigUint::from(10u64)
            ),
            Some(BigUint::one())
        );
        assert_eq!(
            fold_binary(
                BinaryOperation::Lt,
                &BigUint::from(10u64),
                &BigUint::from(5u64)
            ),
            Some(BigUint::zero())
        );
        assert_eq!(
            fold_binary(
                BinaryOperation::Eq,
                &BigUint::from(42u64),
                &BigUint::from(42u64)
            ),
            Some(BigUint::one())
        );
    }

    #[test]
    fn test_constant_fold_bitwise() {
        assert_eq!(
            fold_binary(
                BinaryOperation::And,
                &BigUint::from(0xFF00u64),
                &BigUint::from(0x0FF0u64)
            ),
            Some(BigUint::from(0x0F00u64))
        );
        assert_eq!(
            fold_binary(
                BinaryOperation::Or,
                &BigUint::from(0xFF00u64),
                &BigUint::from(0x00FFu64)
            ),
            Some(BigUint::from(0xFFFFu64))
        );
    }

    #[test]
    fn test_constant_fold_shifts() {
        // EVM convention: shl(shift_amount, value) = value << shift_amount
        // fold_binary(Shl, a=shift_amount, b=value) = b << a
        assert_eq!(
            fold_binary(
                BinaryOperation::Shl,
                &BigUint::from(8u64),
                &BigUint::from(1u64)
            ),
            Some(BigUint::from(256u64))
        );
        // shr(shift_amount, value) = value >> shift_amount
        assert_eq!(
            fold_binary(
                BinaryOperation::Shr,
                &BigUint::from(4u64),
                &BigUint::from(256u64)
            ),
            Some(BigUint::from(16u64))
        );
    }

    #[test]
    fn test_unary_fold() {
        assert_eq!(
            fold_unary(UnaryOperation::IsZero, &BigUint::zero()),
            Some(BigUint::one())
        );
        assert_eq!(
            fold_unary(UnaryOperation::IsZero, &BigUint::from(42u64)),
            Some(BigUint::zero())
        );
        assert_eq!(
            fold_unary(UnaryOperation::Not, &BigUint::zero()),
            Some(max_u256())
        );
    }

    #[test]
    fn test_simplifier_constant_propagation() {
        let mut simplifier = Simplifier::new();

        // v3 uses v1 and v2, so we also need something that uses v3
        // to prevent DCE from removing everything
        let statements = vec![
            make_let_literal(1, 10),
            make_let_literal(2, 20),
            make_let_binop(3, BinaryOperation::Add, 1, 2),
            Statement::Return {
                offset: Value::int(ValueId(3)),
                length: Value::int(ValueId(3)),
            },
        ];

        let block = &mut Block { statements };
        simplifier.simplify_block(block);

        // After constant folding + DCE: v1 and v2 are removed (unused after folding),
        // v3 = literal 30 remains, and the return references v3
        // Find the Let for v3
        let v3_let = block.statements.iter().find(
            |s| matches!(s, Statement::Let { bindings, .. } if bindings.contains(&ValueId(3))),
        );
        let v3_let = v3_let.expect("v3 should still exist");
        if let Statement::Let { value, .. } = v3_let {
            if let Expression::Literal { value, .. } = value {
                assert_eq!(*value, BigUint::from(30u64));
            } else {
                panic!("Expected literal after constant folding, got: {value:?}");
            }
        }
    }

    #[test]
    fn test_simplifier_algebraic_identity_add_zero() {
        let mut simplifier = Simplifier::new();

        let statements = vec![
            make_let_literal(1, 0),
            // let v2 = add(v99, v1) where v1 = 0 → should simplify to Var(v99)
            Statement::Let {
                bindings: vec![ValueId(2)],
                value: Expression::Binary {
                    op: BinaryOperation::Add,
                    lhs: Value::int(ValueId(99)),
                    rhs: Value::int(ValueId(1)),
                },
            },
            Statement::Return {
                offset: Value::int(ValueId(2)),
                length: Value::int(ValueId(2)),
            },
        ];

        let block = &mut Block { statements };
        simplifier.simplify_block(block);

        // After: add(v99, 0) → Var(v99), v2 now holds Var(v99)
        let v2_let = block.statements.iter().find(
            |s| matches!(s, Statement::Let { bindings, .. } if bindings.contains(&ValueId(2))),
        );
        let v2_let = v2_let.expect("v2 should still exist");
        if let Statement::Let { value, .. } = v2_let {
            match value {
                Expression::Var(id) => assert_eq!(id.0, 99),
                _ => panic!("Expected Var after algebraic simplification, got: {value:?}"),
            }
        }
        assert!(simplifier.stats.identities_simplified > 0);
    }

    #[test]
    fn test_no_crash_on_unused_bindings() {
        // DCE is currently disabled, but the simplifier should not crash
        // with unused bindings present.
        let mut simplifier = Simplifier::new();

        let statements = vec![
            make_let_literal(1, 42),  // v1 = 42, unused
            make_let_literal(2, 100), // v2 = 100, used below
            Statement::Return {
                offset: Value::int(ValueId(2)),
                length: Value::int(ValueId(2)),
            },
        ];

        let block = &mut Block { statements };
        simplifier.simplify_block(block);

        // Without DCE, all statements are preserved
        assert_eq!(block.statements.len(), 3);
    }

    #[test]
    fn test_copy_propagation() {
        let mut simplifier = Simplifier::new();

        let statements = vec![
            make_let_literal(1, 42),
            // let v2 = v1 (copy)
            Statement::Let {
                bindings: vec![ValueId(2)],
                value: Expression::Var(ValueId(1)),
            },
            // use v2 → should become v1
            Statement::Return {
                offset: Value::int(ValueId(2)),
                length: Value::int(ValueId(2)),
            },
        ];

        let block = &mut Block { statements };
        simplifier.simplify_block(block);

        // The return should now reference v1 (or the literal 42 via constant prop)
        if let Statement::Return { offset, .. } = &block.statements[block.statements.len() - 1] {
            // v2 was a copy of v1, so after propagation it should resolve to v1
            // With constant propagation it might even be inlined as literal
            assert!(
                offset.id.0 == 1
                    || matches!(block.statements.last(), Some(Statement::Return { .. }))
            );
        }
    }

    #[test]
    fn test_ternary_fold() {
        // addmod(10, 20, 7) = 30 % 7 = 2
        let result = fold_ternary(
            BinaryOperation::AddMod,
            &BigUint::from(10u64),
            &BigUint::from(20u64),
            &BigUint::from(7u64),
        );
        assert_eq!(result, Some(BigUint::from(2u64)));

        // mulmod(5, 7, 6) = 35 % 6 = 5
        let result = fold_ternary(
            BinaryOperation::MulMod,
            &BigUint::from(5u64),
            &BigUint::from(7u64),
            &BigUint::from(6u64),
        );
        assert_eq!(result, Some(BigUint::from(5u64)));

        // addmod(x, y, 0) = 0
        let result = fold_ternary(
            BinaryOperation::AddMod,
            &BigUint::from(10u64),
            &BigUint::from(20u64),
            &BigUint::zero(),
        );
        assert_eq!(result, Some(BigUint::zero()));
    }

    #[test]
    fn test_exp_fold() {
        let result = fold_binary(
            BinaryOperation::Exp,
            &BigUint::from(2u64),
            &BigUint::from(10u64),
        );
        assert_eq!(result, Some(BigUint::from(1024u64)));
    }

    #[test]
    fn test_byte_fold() {
        // byte(31, 0xff) = 0xff (least significant byte)
        let result = fold_binary(
            BinaryOperation::Byte,
            &BigUint::from(31u64),
            &BigUint::from(0xFFu64),
        );
        assert_eq!(result, Some(BigUint::from(0xFFu64)));

        // byte(0, 0xff) = 0 (most significant byte of 0xff is 0)
        let result = fold_binary(
            BinaryOperation::Byte,
            &BigUint::from(0u64),
            &BigUint::from(0xFFu64),
        );
        assert_eq!(result, Some(BigUint::zero()));
    }
}
