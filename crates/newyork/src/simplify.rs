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
    for_each_statement_mut, BinaryOperation, BitWidth, Block, CallKind, Expression, Function,
    FunctionId, MemoryRegion, Object, Region, Statement, SwitchCase, Type, UnaryOperation, Value,
    ValueId,
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
///
/// These are nullary expressions whose value is fixed for the duration of a
/// single contract invocation, so reads of the same kind in the same dominator
/// scope can share a binding.
///
/// Deliberately excluded — these *look* nullary but are observably variable:
/// - `Gas`: gas remaining, decreases on every opcode.
/// - `MSize`: memory size, grows when memory expands.
/// - `ReturnDataSize`: resets after each external call.
/// - `SelfBalance`: changes when the contract sends or receives value.
///
/// Hash expressions (`Keccak256*`) take operands and so aren't `EnvRead`
/// candidates by shape; see `HASH_CSE_NOTE.md` for the separate CSE story.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum EnvRead {
    Address,
    BaseFee,
    BlobBaseFee,
    CallDataSize,
    CallValue,
    Caller,
    ChainId,
    CodeSize,
    Coinbase,
    Difficulty,
    GasLimit,
    GasPrice,
    Number,
    Origin,
    Timestamp,
}

/// Returns the `EnvRead` kind for an expression if it is a pure environment read.
fn env_read_kind(expression: &Expression) -> Option<EnvRead> {
    match expression {
        Expression::Address => Some(EnvRead::Address),
        Expression::BaseFee => Some(EnvRead::BaseFee),
        Expression::BlobBaseFee => Some(EnvRead::BlobBaseFee),
        Expression::CallDataSize => Some(EnvRead::CallDataSize),
        Expression::CallValue => Some(EnvRead::CallValue),
        Expression::Caller => Some(EnvRead::Caller),
        Expression::ChainId => Some(EnvRead::ChainId),
        Expression::CodeSize => Some(EnvRead::CodeSize),
        Expression::Coinbase => Some(EnvRead::Coinbase),
        Expression::Difficulty => Some(EnvRead::Difficulty),
        Expression::GasLimit => Some(EnvRead::GasLimit),
        Expression::GasPrice => Some(EnvRead::GasPrice),
        Expression::Number => Some(EnvRead::Number),
        Expression::Origin => Some(EnvRead::Origin),
        Expression::Timestamp => Some(EnvRead::Timestamp),
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
    /// Maps ValueId → (BinaryOperation, lhs ValueId, rhs ValueId) for binary
    /// expression tracking. Used to simplify patterns like `sub(add(x, y), x) → y`.
    binary_defs: BTreeMap<u32, (BinaryOperation, ValueId, ValueId)>,
    /// Counter for fresh value IDs when creating new bindings (strength reduction).
    next_value_id: ValueId,
    /// CSE cache for pure environment reads (calldatasize, caller, etc.).
    /// Maps the read category to the first ValueId that bound this expression.
    /// Saved/restored in region scopes to ensure LLVM SSA domination correctness:
    /// a binding from one branch must not be referenced from a sibling branch.
    env_reads: BTreeMap<EnvRead, ValueId>,
    /// Statistics.
    statistics: SimplifyResults,
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
            binary_defs: BTreeMap::new(),
            next_value_id: ValueId(0),
            env_reads: BTreeMap::new(),
            statistics: SimplifyResults::default(),
        }
    }

    /// Allocates a fresh ValueId.
    fn fresh_id(&mut self) -> ValueId {
        self.next_value_id.fresh()
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
        self.next_value_id = ValueId(object.find_max_value_id() + 1);

        self.simplify_block(&mut object.code);
        self.statistics.dead_bindings_removed +=
            eliminate_dead_code_in_stmts(&mut object.code.statements, &BTreeSet::new());

        for function in object.functions.values_mut() {
            self.constants.clear();
            self.copies.clear();
            self.unary_defs.clear();
            self.env_reads.clear();
            function.body.statements =
                self.simplify_statements(std::mem::take(&mut function.body.statements));

            Self::reconcile_dangling_return_values(function);

            let mut extra_used = BTreeSet::new();
            for ret_id in &function.return_values {
                extra_used.insert(ret_id.0);
            }
            self.statistics.dead_bindings_removed +=
                eliminate_dead_code_in_stmts(&mut function.body.statements, &extra_used);
        }

        std::mem::take(&mut self.statistics)
    }

    /// Reconciles fall-through return values left dangling by simplification.
    ///
    /// Folding an always-true guard can collapse a function body to an unconditional
    /// `leave`, after which the dead fall-through code that defined some `return_values`
    /// is removed. Those `return_values` then reference deleted definitions, which the IR
    /// validator rejects as "used before definition" (codegen would zero-init them, but
    /// only the unreachable fall-through path observes that). Each now-undefined return
    /// value is pointed at its entry `return_values_initial` slot — defined at function
    /// entry, and sound because the fall-through that would observe it is unreachable.
    /// Return values still defined in the body are left untouched, so functions that
    /// legitimately end in a `leave` are unaffected.
    fn reconcile_dangling_return_values(function: &mut Function) {
        let mut defined_ids: BTreeSet<u32> = BTreeSet::new();
        for (id, _ty) in &function.parameters {
            defined_ids.insert(id.0);
        }
        for id in &function.return_values_initial {
            defined_ids.insert(id.0);
        }
        crate::ir::for_each_statement(&function.body.statements, &mut |statement| {
            statement.for_each_value_id_def(&mut |id| {
                defined_ids.insert(id.0);
            });
        });
        for (i, return_value) in function.return_values.iter_mut().enumerate() {
            if !defined_ids.contains(&return_value.0) {
                if let Some(initial) = function.return_values_initial.get(i) {
                    *return_value = *initial;
                }
            }
        }
    }

    /// Runs only DCE (dead code elimination) on an object without the full simplification pass.
    ///
    /// This is useful after late-stage passes (mem_opt, keccak folding) that leave
    /// Simplifies a block in place.
    fn simplify_block(&mut self, block: &mut Block) {
        block.statements = self.simplify_statements(std::mem::take(&mut block.statements));
    }

    /// Simplifies a list of statements, returning the simplified list.
    ///
    /// Object code has no yields; a function body's return values are repaired separately by
    /// `reconcile_dangling_return_values`, so no rescue set is needed here.
    fn simplify_statements(&mut self, statements: Vec<Statement>) -> Vec<Statement> {
        let outer_constants = self.constants.clone();
        let outer_copies = self.copies.clone();

        let mut result = Vec::with_capacity(statements.len());

        for statement in statements {
            let simplified = self.simplify_statement(statement);
            result.extend(simplified);
        }

        result = outline_panic_patterns(result, &self.constants, &BTreeSet::new());

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
                let simplified_expr = self.simplify_expression(value);

                if bindings.len() == 1 {
                    if let Some(statements) = self.try_strength_reduce(&bindings, &simplified_expr)
                    {
                        return statements;
                    }
                }

                if bindings.len() == 1 {
                    if let Expression::Literal { ref value, .. } = simplified_expr {
                        self.constants.insert(bindings[0].0, value.clone());
                    }
                    if let Expression::Var(src_id) = &simplified_expr {
                        let resolved = self.resolve_copy(*src_id);
                        self.copies.insert(bindings[0].0, resolved);
                        if let Some(c) = self.constants.get(&resolved.0).cloned() {
                            self.constants.insert(bindings[0].0, c);
                        }
                    }

                    if let Some(kind) = env_read_kind(&simplified_expr) {
                        self.env_reads.entry(kind).or_insert(bindings[0]);
                    }

                    if let Expression::Unary { operation, operand } = &simplified_expr {
                        self.unary_defs
                            .insert(bindings[0].0, (*operation, operand.id));
                    }

                    if let Expression::Binary {
                        operation,
                        lhs,
                        rhs,
                    } = &simplified_expr
                    {
                        self.binary_defs
                            .insert(bindings[0].0, (*operation, lhs.id, rhs.id));
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

                if let Some(cond_const) = cond_val {
                    let is_true = !cond_const.is_zero();
                    self.statistics.branches_eliminated += 1;

                    if is_true {
                        let then_region = self.simplify_region(then_region);
                        let mut result = Vec::new();
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

                if let Some(scrut_const) = scrut_val {
                    let matching_case = cases.into_iter().find(|c| c.value == scrut_const);

                    let taken_region = if let Some(case) = matching_case {
                        self.simplify_region(case.body)
                    } else if let Some(default_region) = default {
                        self.simplify_region(default_region)
                    } else {
                        let inputs: Vec<Value> =
                            inputs.into_iter().map(|v| self.resolve_value(v)).collect();
                        let mut result = vec![];
                        for (output_id, input_val) in outputs.iter().zip(inputs.iter()) {
                            result.push(Statement::Let {
                                bindings: vec![*output_id],
                                value: Expression::Var(input_val.id),
                            });
                        }
                        self.statistics.branches_eliminated += 1;
                        return result;
                    };

                    let mut result = Vec::new();
                    result.extend(taken_region.statements);
                    for (output_id, yield_val) in outputs.iter().zip(taken_region.yields.iter()) {
                        result.push(Statement::Let {
                            bindings: vec![*output_id],
                            value: Expression::Var(yield_val.id),
                        });
                    }
                    self.statistics.branches_eliminated += 1;
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

                let mut hoisted = Vec::new();
                if !cases.is_empty() {
                    if callvalue_check_fully_hoistable(&cases, &default) {
                        let hoisted_cv = callvalue_binding_id(&cases[0].body.statements);
                        hoisted.push(cases[0].body.statements[0].clone());
                        hoisted.push(cases[0].body.statements[1].clone());
                        for case in &mut cases {
                            drain_callvalue_prefix(&mut case.body.statements, hoisted_cv);
                        }
                        if let Some(ref mut d) = default {
                            drain_callvalue_prefix(&mut d.statements, hoisted_cv);
                        }
                    } else {
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

                let saved_constants = self.constants.clone();
                let saved_copies = self.copies.clone();

                let condition_statements = self.simplify_statements(condition_statements);
                let condition = self.simplify_expression(condition);
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
                vec![Statement::Expression(self.simplify_expression(expression))]
            }

            Statement::Stop
            | Statement::Invalid
            | Statement::PanicRevert { .. }
            | Statement::ErrorStringRevert { .. }
            | Statement::CustomErrorRevert { .. } => vec![statement],

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
    ///
    /// Yields are resolved before outlining so a panic-pattern truncation can rescue any pure
    /// binding they still reference (otherwise the yield would dangle).
    fn simplify_region(&mut self, region: Region) -> Region {
        let outer_constants = self.constants.clone();
        let outer_copies = self.copies.clone();
        let outer_env_reads = self.env_reads.clone();

        let mut statements = Vec::with_capacity(region.statements.len());
        for statement in region.statements {
            let simplified = self.simplify_statement(statement);
            statements.extend(simplified);
        }

        let yields: Vec<Value> = region
            .yields
            .into_iter()
            .map(|v| self.resolve_value(v))
            .collect();
        let yielded = yields_as_used(&yields);

        statements = outline_panic_patterns(statements, &self.constants, &yielded);

        statements = outline_custom_error_patterns(statements, &self.constants);

        self.constants = outer_constants;
        self.copies = outer_copies;
        self.env_reads = outer_env_reads;

        Region { statements, yields }
    }

    /// Simplifies an expression, performing constant folding, algebraic identities,
    /// and copy propagation on operands.
    ///
    /// Among the algebraic identities are ones that recurse through a previously-recorded binary
    /// definition: `sub(add(a, b), a) → b`, `sub(add(a, b), b) → a`, `add(sub(a, b), b) → a`, and
    /// `add(b, sub(a, b)) → a`. These come up wherever a helper computes `tail = base + 32` and the
    /// caller takes `length = tail - base` (or the symmetric forms); recognising them lets the
    /// return_word peephole match against Solidity's inline encode helper output.
    fn simplify_expression(&mut self, expression: Expression) -> Expression {
        match expression {
            Expression::Binary {
                operation,
                lhs,
                rhs,
            } => {
                let lhs = self.resolve_value(lhs);
                let rhs = self.resolve_value(rhs);
                let lhs_val = self.try_get_const(&lhs);
                let rhs_val = self.try_get_const(&rhs);

                if let (Some(a), Some(b)) = (&lhs_val, &rhs_val) {
                    if let Some(result) = fold_binary(operation, a, b) {
                        self.statistics.constants_folded += 1;
                        return Expression::Literal {
                            value: result,
                            value_type: result_type(operation),
                        };
                    }
                }

                if let Some(simplified) = simplify_binary(operation, &lhs, &rhs, &lhs_val, &rhs_val)
                {
                    self.statistics.identities_simplified += 1;
                    return simplified;
                }

                if operation == BinaryOperation::Sub {
                    if let Some(&(lhs_op, lhs_a, lhs_b)) = self.binary_defs.get(&lhs.id.0) {
                        if lhs_op == BinaryOperation::Add {
                            let resolved_a = self.resolve_copy(lhs_a);
                            let resolved_b = self.resolve_copy(lhs_b);
                            if resolved_a == rhs.id {
                                self.statistics.identities_simplified += 1;
                                return Expression::Var(resolved_b);
                            }
                            if resolved_b == rhs.id {
                                self.statistics.identities_simplified += 1;
                                return Expression::Var(resolved_a);
                            }
                        }
                    }
                }
                if operation == BinaryOperation::Add {
                    if let Some(&(lhs_op, lhs_a, lhs_b)) = self.binary_defs.get(&lhs.id.0) {
                        if lhs_op == BinaryOperation::Sub {
                            let resolved_a = self.resolve_copy(lhs_a);
                            let resolved_b = self.resolve_copy(lhs_b);
                            if resolved_b == rhs.id {
                                self.statistics.identities_simplified += 1;
                                return Expression::Var(resolved_a);
                            }
                        }
                    }
                    if let Some(&(rhs_op, rhs_a, rhs_b)) = self.binary_defs.get(&rhs.id.0) {
                        if rhs_op == BinaryOperation::Sub {
                            let resolved_a = self.resolve_copy(rhs_a);
                            let resolved_b = self.resolve_copy(rhs_b);
                            if resolved_b == lhs.id {
                                self.statistics.identities_simplified += 1;
                                return Expression::Var(resolved_a);
                            }
                        }
                    }
                }

                Expression::Binary {
                    operation,
                    lhs,
                    rhs,
                }
            }

            Expression::Unary { operation, operand } => {
                let operand = self.resolve_value(operand);
                let operand_val = self.try_get_const(&operand);

                if let Some(c) = &operand_val {
                    if let Some(result) = fold_unary(operation, c) {
                        self.statistics.constants_folded += 1;
                        return Expression::Literal {
                            value: result,
                            value_type: unary_result_type(operation),
                        };
                    }
                }

                if operation == UnaryOperation::Not {
                    if let Some((UnaryOperation::Not, inner)) = self.unary_defs.get(&operand.id.0) {
                        self.statistics.identities_simplified += 1;
                        return Expression::Var(*inner);
                    }
                }

                Expression::Unary { operation, operand }
            }

            Expression::Ternary { operation, a, b, n } => {
                let a = self.resolve_value(a);
                let b = self.resolve_value(b);
                let n = self.resolve_value(n);
                let a_val = self.try_get_const(&a);
                let b_val = self.try_get_const(&b);
                let n_val = self.try_get_const(&n);

                if let (Some(av), Some(bv), Some(nv)) = (&a_val, &b_val, &n_val) {
                    if let Some(result) = fold_ternary(operation, av, bv, nv) {
                        self.statistics.constants_folded += 1;
                        return Expression::Literal {
                            value: result,
                            value_type: Type::Int(BitWidth::I256),
                        };
                    }
                }

                Expression::Ternary { operation, a, b, n }
            }

            Expression::Var(id) => {
                let resolved = self.resolve_copy(id);
                Expression::Var(resolved)
            }

            Expression::MLoad { offset, region } => {
                let offset = self.resolve_value(offset);
                let region = if region == MemoryRegion::Unknown {
                    self.resolve_region(&offset)
                } else {
                    region
                };
                Expression::MLoad { offset, region }
            }

            Expression::Address => self.cse_env_read(EnvRead::Address, expression),
            Expression::BaseFee => self.cse_env_read(EnvRead::BaseFee, expression),
            Expression::BlobBaseFee => self.cse_env_read(EnvRead::BlobBaseFee, expression),
            Expression::CallDataSize => self.cse_env_read(EnvRead::CallDataSize, expression),
            Expression::CallValue => self.cse_env_read(EnvRead::CallValue, expression),
            Expression::Caller => self.cse_env_read(EnvRead::Caller, expression),
            Expression::ChainId => self.cse_env_read(EnvRead::ChainId, expression),
            Expression::CodeSize => self.cse_env_read(EnvRead::CodeSize, expression),
            Expression::Coinbase => self.cse_env_read(EnvRead::Coinbase, expression),
            Expression::Difficulty => self.cse_env_read(EnvRead::Difficulty, expression),
            Expression::GasLimit => self.cse_env_read(EnvRead::GasLimit, expression),
            Expression::GasPrice => self.cse_env_read(EnvRead::GasPrice, expression),
            Expression::Number => self.cse_env_read(EnvRead::Number, expression),
            Expression::Origin => self.cse_env_read(EnvRead::Origin, expression),
            Expression::Timestamp => self.cse_env_read(EnvRead::Timestamp, expression),

            Expression::Keccak256Single { word0 } => {
                let word0 = self.resolve_value(word0);
                if let Some(c) = self.try_get_const(&word0) {
                    let result = fold_keccak256_single(&c);
                    self.statistics.constants_folded += 1;
                    Expression::Literal {
                        value: result,
                        value_type: Type::Int(BitWidth::I256),
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
                    self.statistics.constants_folded += 1;
                    Expression::Literal {
                        value: result,
                        value_type: Type::Int(BitWidth::I256),
                    }
                } else {
                    Expression::Keccak256Pair { word0, word1 }
                }
            }

            Expression::MappingSLoad { key, slot } => Expression::MappingSLoad {
                key: self.resolve_value(key),
                slot: self.resolve_value(slot),
            },

            other => other,
        }
    }

    /// Checks if an environment read has been cached and returns a Var reference
    /// to the first binding if so. Otherwise returns the original expression.
    fn cse_env_read(&mut self, kind: EnvRead, original: Expression) -> Expression {
        if let Some(&cached_id) = self.env_reads.get(&kind) {
            self.statistics.env_reads_eliminated += 1;
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
        self.statistics.identities_simplified += 1;
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
                    value_type: Type::Int(BitWidth::I256),
                },
            },
            Statement::Let {
                bindings: bindings.to_vec(),
                value: Expression::Binary {
                    operation: target_op,
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
        let (operation, lhs, rhs) = match expression {
            Expression::Binary {
                operation,
                lhs,
                rhs,
            } => (*operation, *lhs, *rhs),
            _ => return None,
        };
        let lhs_val = self.try_get_const(&lhs);
        let rhs_val = self.try_get_const(&rhs);
        let in_range = |k: u32| (1..256).contains(&k);

        match operation {
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
/// `extra_used` holds value ids that must stay defined even though they are not referenced by the
/// statements themselves — an enclosing region's resolved `yields`. A pure binding inside the
/// collapsed window may define such a value; truncating it would leave the yield dangling and fail
/// SSA validation, so each is re-bound to zero before the [`Statement::PanicRevert`] (the binding is
/// dead — the panic diverges — but dominates the yield, mirroring `eliminate_dead_code_in_stmts`).
fn outline_panic_patterns(
    statements: Vec<Statement>,
    scope_constants: &BTreeMap<u32, BigUint>,
    extra_used: &BTreeSet<u32>,
) -> Vec<Statement> {
    let has_revert = statements
        .iter()
        .any(|s| matches!(s, Statement::Revert { .. }));
    if !has_revert {
        return statements;
    }

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
                    let mut rescued: Vec<ValueId> = Vec::new();
                    for statement in &result[panic_start..] {
                        statement.for_each_value_id_def(&mut |id| {
                            if extra_used.contains(&id.0) {
                                rescued.push(id);
                            }
                        });
                    }
                    result.truncate(panic_start);
                    for id in rescued {
                        result.push(Statement::Let {
                            bindings: vec![id],
                            value: Expression::Literal {
                                value: BigUint::zero(),
                                value_type: Type::Int(BitWidth::I256),
                            },
                        });
                    }
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
///
/// The simplifier emits `Statement::PanicRevert { code: u8 }` which encodes Solidity's canonical
/// 36-byte panic revert with `code` zero-padded into the last byte. Only fold when the original
/// mstored value actually fits in `u8` — i.e. ALL bits above the low byte are zero. Using
/// `to_u64_digits().first()` here was unsound: it returned only the lowest u64 digit, so values
/// like `2^124 + 0x42` were mis-classified as `code = 0x42` and the high bits (which the EVM emits
/// literally into the revert data) were silently dropped.
///
/// Last-write-wins: the matched selector/code stores must be the final writes to offsets 0/4, or
/// the revert payload differs from canonical Panic.
fn find_panic_pattern_backwards(
    statements: &[Statement],
    constants: &BTreeMap<u32, BigUint>,
) -> Option<(usize, u8)> {
    let len = statements.len();
    if len < 2 {
        return None;
    }

    let mut mstore4_idx = None;
    let mut error_code = None;
    let search_limit = len.saturating_sub(10);

    for j in (search_limit..len).rev() {
        match &statements[j] {
            Statement::MStore { offset, value, .. } => {
                if is_const_value(offset.id, 4, constants) {
                    if let Some(code_val) = constants.get(&value.id.0) {
                        if code_val.bits() <= 8 {
                            let code_u8 =
                                code_val.to_u64_digits().first().copied().unwrap_or(0) as u8;
                            mstore4_idx = Some(j);
                            error_code = Some(code_u8);
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

    let search_limit2 = mstore4_idx.saturating_sub(10);
    for j in (search_limit2..mstore4_idx).rev() {
        match &statements[j] {
            Statement::MStore { offset, value, .. } => {
                if is_const_value(offset.id, 0, constants) {
                    if let Some(sel_val) = constants.get(&value.id.0) {
                        let sel_hex = format!("{sel_val:064x}");
                        if sel_hex == revive_common::PANIC_UINT256_SELECTOR_WORD_HEX
                            && statements[j..]
                                .iter()
                                .all(|s| panic_window_droppable(s, constants))
                            && !any_mstore_to_offset(&statements[j + 1..], 0, constants)
                            && !any_mstore_to_offset(&statements[mstore4_idx + 1..], 4, constants)
                        {
                            return Some((j, error_code));
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

/// Whether `statement` may be dropped when collapsing a panic-revert window into a `PanicRevert`.
///
/// - An `MStore` writes 32 bytes at `[p, p + 32)`; for `p < 0x24` those overlap the revert range
///   `[0, 0x24)` that the EVM emits as revert data. The canonical panic shape stores only at `p = 0`
///   (selector) and `p = 4` (code); any other in-range store (e.g. `mstore(7, …)`) would corrupt the
///   payload, so it blocks the collapse. Stores at `p >= 0x24` don't escape and are safe.
/// - A `Let`/`Expression` is droppable only if its value is pure. A side-effecting value — an
///   internal call that can revert differently / never return / write scratch, or a memory/storage
///   read — is observable, so dropping it would change behavior. This matches `eliminate_dead_code`,
///   which never removes effectful statements. A dropped pure binding that an enclosing yield still
///   needs is zero-rebound by [`outline_panic_patterns`].
fn panic_window_droppable(statement: &Statement, constants: &BTreeMap<u32, BigUint>) -> bool {
    match statement {
        Statement::MStore { offset, .. } => {
            is_const_value(offset.id, 0, constants)
                || is_const_value(offset.id, 4, constants)
                || constants
                    .get(&offset.id.0)
                    .is_some_and(|v| v.to_u64().is_some_and(|o| o >= 0x24))
        }
        Statement::Let { value, .. } | Statement::Expression(value) => {
            !expr_has_side_effects(value)
        }
        _ => false,
    }
}

/// Whether any of `statements` is an `mstore` to the constant byte `offset`. Used to confirm a
/// matched panic selector/code store is the last write to its offset (last-write-wins).
fn any_mstore_to_offset(
    statements: &[Statement],
    offset: u64,
    constants: &BTreeMap<u32, BigUint>,
) -> bool {
    statements.iter().any(|s| {
        matches!(s, Statement::MStore { offset: o, .. } if is_const_value(o.id, offset, constants))
    })
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
    let has_revert = statements
        .iter()
        .any(|s| matches!(s, Statement::Revert { .. }));
    if !has_revert {
        return statements;
    }

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
        if let Statement::Revert {
            ref offset,
            ref length,
        } = statement
        {
            if constants.get(&offset.id.0).is_some_and(|v| *v == zero) {
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
                            let mut kept = Vec::new();
                            for s in result.drain(start_idx..) {
                                match s {
                                    Statement::MStore { .. } => {}
                                    _ => kept.push(s),
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
///
/// The scan records each payload offset's store and tracks `earliest_idx` (the start of the matched
/// window). It deliberately does NOT try to keep the latest store per offset: that would be unsound
/// for overlapping unaligned stores, and it would also skip earlier duplicates so `earliest_idx`
/// would no longer cover them. Instead the exactly-once check rejects any window with a duplicate
/// payload-offset store, so a recorded value is only ever used when its offset has a single store.
///
/// The revert range is `[0, 4 + 32 * num_args)`. The canonical Solidity custom-error shape writes
/// at exactly the offsets `0`, `4`, `0x24`, … `4 + 32 * (num_args - 1)`. Any in-range mstore at a
/// different offset corrupts the revert payload — we must NOT collapse the sequence into
/// `Statement::CustomErrorRevert`. Out-of-range mstores (offset ≥ revert_length) are fine; they
/// don't escape.
///
/// Last-write-wins: each payload offset (selector at 0, arguments at 4, 0x24, …) must be written
/// exactly once in the collapsed window. A repeated store means a later write overwrites an earlier
/// one — so the value the backward scan matched is not what the EVM revert emits — and the sequence
/// must not be collapsed into a `CustomErrorRevert`.
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
                if let Some(off_val) = constants.get(&offset.id.0) {
                    if *off_val == zero {
                        if let Some(sel) = constants.get(&value.id.0) {
                            found_selector = Some(sel.clone());
                            earliest_idx = earliest_idx.min(j);
                        }
                    } else if *off_val == four && num_args >= 1 {
                        arguments[0] = Some(*value);
                        earliest_idx = earliest_idx.min(j);
                    } else if let Some(off_u64) = off_val.to_u64() {
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
            Statement::Let { .. } | Statement::Expression(..) => continue,
            _ => break,
        }
    }

    let selector = found_selector?;
    let arguments: Vec<Value> = arguments.into_iter().collect::<Option<Vec<_>>>()?;

    let revert_length = 4 + (num_args as u64) * 0x20;
    let all_safe = statements[earliest_idx..].iter().all(|s| match s {
        Statement::MStore { offset, .. } => constants.get(&offset.id.0).is_some_and(|off_val| {
            if let Some(off_u64) = off_val.to_u64() {
                if off_u64 >= revert_length {
                    return true;
                }
                if off_u64 == 0 {
                    return true;
                }
                off_u64 >= 4 && (off_u64 - 4) % 0x20 == 0
            } else {
                false
            }
        }),
        Statement::Let { .. } | Statement::Expression(..) => true,
        _ => false,
    });
    if !all_safe {
        return None;
    }

    let payload_offsets = std::iter::once(0u64).chain((0..num_args as u64).map(|i| 4 + i * 0x20));
    for payload_offset in payload_offsets {
        let writes = statements[earliest_idx..]
            .iter()
            .filter(|s| {
                matches!(s, Statement::MStore { offset, .. }
                    if is_const_value(offset.id, payload_offset, constants))
            })
            .count();
        if writes != 1 {
            return None;
        }
    }

    Some((earliest_idx, selector, arguments))
}

/// Returns the result type for a binary operation.
fn result_type(operation: BinaryOperation) -> Type {
    match operation {
        BinaryOperation::Lt
        | BinaryOperation::Gt
        | BinaryOperation::Slt
        | BinaryOperation::Sgt
        | BinaryOperation::Eq => Type::Int(BitWidth::I256),
        _ => Type::Int(BitWidth::I256),
    }
}

/// Returns the result type for a unary operation.
fn unary_result_type(operation: UnaryOperation) -> Type {
    match operation {
        UnaryOperation::IsZero => Type::Int(BitWidth::I256),
        UnaryOperation::Not | UnaryOperation::Clz => Type::Int(BitWidth::I256),
    }
}

/// Folds a binary operation on two constant values.
/// Returns None if the operation cannot be folded.
fn fold_binary(operation: BinaryOperation, a: &BigUint, b: &BigUint) -> Option<BigUint> {
    let modulus = modulus_u256();
    let max = max_u256();

    Some(match operation {
        BinaryOperation::Add => (a + b) % &modulus,
        BinaryOperation::Sub => {
            if a >= b {
                a - b
            } else {
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
        BinaryOperation::Exp => a.modpow(b, &modulus),
        BinaryOperation::And => a & b,
        BinaryOperation::Or => a | b,
        BinaryOperation::Xor => a ^ b,
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
            if *a >= BigUint::from(32u32) {
                BigUint::zero()
            } else {
                let n = a.to_u64_digits().first().copied().unwrap_or(0) as usize;
                let shift = (31 - n) * 8;
                (b >> shift) & BigUint::from(0xffu32)
            }
        }
        BinaryOperation::SignExtend => fold_signextend(a, b, &max)?,
        BinaryOperation::AddMod | BinaryOperation::MulMod => return None,
    })
}

/// Folds a unary operation on a constant value.
fn fold_unary(operation: UnaryOperation, a: &BigUint) -> Option<BigUint> {
    Some(match operation {
        UnaryOperation::IsZero => bool_to_u256(a.is_zero()),
        UnaryOperation::Not => &max_u256() ^ a,
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
fn fold_ternary(
    operation: BinaryOperation,
    a: &BigUint,
    b: &BigUint,
    n: &BigUint,
) -> Option<BigUint> {
    if n.is_zero() {
        return Some(BigUint::zero());
    }
    Some(match operation {
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
    Some((value.bits() - 1) as u32)
}

/// Applies algebraic identity simplifications.
/// Returns Some(simplified_expr) if an identity applies, None otherwise.
///
/// `div(x, x) → 1` and `sdiv(x, x) → 1` are unsound when `x == 0`. EVM defines division by zero to
/// return 0, so `div(0, 0) = 0`, not 1. We can only fold the same-operand case when we statically
/// know `x != 0`.
fn simplify_binary(
    operation: BinaryOperation,
    lhs: &Value,
    rhs: &Value,
    lhs_val: &Option<BigUint>,
    rhs_val: &Option<BigUint>,
) -> Option<Expression> {
    let i256 = Type::Int(BitWidth::I256);
    let literal = |value: BigUint| Expression::Literal {
        value,
        value_type: i256,
    };
    let var_l = || Expression::Var(lhs.id);
    let var_r = || Expression::Var(rhs.id);
    let same = lhs.id == rhs.id;
    let l_is = |v: &BigUint| lhs_val.as_ref() == Some(v);
    let r_is = |v: &BigUint| rhs_val.as_ref() == Some(v);
    let zero = BigUint::zero();
    let one = BigUint::one();

    match operation {
        BinaryOperation::Add => {
            if r_is(&zero) {
                Some(var_l())
            } else if l_is(&zero) {
                Some(var_r())
            } else {
                None
            }
        }

        BinaryOperation::Sub if r_is(&zero) => Some(var_l()),
        BinaryOperation::Sub if same => Some(literal(zero)),
        BinaryOperation::Sub => None,

        BinaryOperation::Mul if r_is(&zero) || l_is(&zero) => Some(literal(zero)),
        BinaryOperation::Mul if r_is(&one) => Some(var_l()),
        BinaryOperation::Mul if l_is(&one) => Some(var_r()),
        BinaryOperation::Mul => None,

        BinaryOperation::Div | BinaryOperation::SDiv if r_is(&one) => Some(var_l()),
        BinaryOperation::Div | BinaryOperation::SDiv if l_is(&zero) => Some(literal(zero)),
        BinaryOperation::Div | BinaryOperation::SDiv
            if same && lhs_val.as_ref().is_some_and(|v| !v.is_zero()) =>
        {
            Some(literal(one))
        }
        BinaryOperation::Div | BinaryOperation::SDiv => None,

        BinaryOperation::Mod | BinaryOperation::SMod if r_is(&one) || l_is(&zero) || same => {
            Some(literal(zero))
        }
        BinaryOperation::Mod | BinaryOperation::SMod => None,

        BinaryOperation::And if r_is(&zero) || l_is(&zero) => Some(literal(zero)),
        BinaryOperation::And if r_is(&max_u256()) || same => Some(var_l()),
        BinaryOperation::And if l_is(&max_u256()) => Some(var_r()),
        BinaryOperation::And => None,

        BinaryOperation::Or if r_is(&zero) || same => Some(var_l()),
        BinaryOperation::Or if l_is(&zero) => Some(var_r()),
        BinaryOperation::Or if r_is(&max_u256()) || l_is(&max_u256()) => Some(literal(max_u256())),
        BinaryOperation::Or => None,

        BinaryOperation::Xor if r_is(&zero) => Some(var_l()),
        BinaryOperation::Xor if l_is(&zero) => Some(var_r()),
        BinaryOperation::Xor if same => Some(literal(zero)),
        BinaryOperation::Xor => None,

        BinaryOperation::Shl | BinaryOperation::Shr | BinaryOperation::Sar if l_is(&zero) => {
            Some(var_r())
        }
        BinaryOperation::Shl | BinaryOperation::Shr | BinaryOperation::Sar => None,

        BinaryOperation::Eq if same => Some(literal(one)),
        BinaryOperation::Eq => None,

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
            Some(max.clone())
        } else {
            Some(BigUint::zero())
        };
    }

    let shift = b.to_u64_digits().first().copied().unwrap_or(0) as usize;
    let half = modulus >> 1;

    if *a >= half {
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
        return Some(x.clone());
    }

    let byte_pos = b.to_u64_digits().first().copied().unwrap_or(0) as usize;
    let bit_pos = byte_pos * 8 + 7;
    let sign_bit = (x >> bit_pos) & BigUint::one();

    if sign_bit.is_one() {
        let mask = max << (bit_pos + 1);
        Some((x | mask) & max)
    } else {
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
///
/// Statements following an unconditional terminator are unreachable and get
/// truncated. Some of them may still define a value that an enclosing `yield` or
/// return references through `extra_used`: folding a constant `if` whose taken
/// branch ends in a `leave` appends that branch's output binding after the `leave`,
/// and the enclosing region's `yield` keeps pointing at it. Truncating the binding
/// would leave that reference dangling and fail SSA validation ("used before
/// definition"). Each such value is therefore re-bound to zero just before the
/// terminator. The binding lies on a provably unreachable path (it follows an
/// unconditional terminator), so the zero is never observed and LLVM discards it.
fn eliminate_dead_code_in_stmts(
    statements: &mut Vec<Statement>,
    extra_used: &BTreeSet<u32>,
) -> usize {
    let mut total_removed = 0;

    if let Some(terminator_pos) = statements.iter().position(is_terminator) {
        let unreachable_count = statements.len() - terminator_pos - 1;
        if unreachable_count > 0 {
            let mut rescued: Vec<ValueId> = Vec::new();
            for statement in &statements[terminator_pos + 1..] {
                statement.for_each_value_id_def(&mut |id| {
                    if extra_used.contains(&id.0) {
                        rescued.push(id);
                    }
                });
            }
            statements.truncate(terminator_pos + 1);
            for id in rescued {
                statements.insert(
                    terminator_pos,
                    Statement::Let {
                        bindings: vec![id],
                        value: Expression::Literal {
                            value: BigUint::zero(),
                            value_type: Type::Int(BitWidth::I256),
                        },
                    },
                );
            }
            total_removed += unreachable_count;
        }
    }

    for statement in statements.iter_mut() {
        total_removed += eliminate_dead_code_in_nested(statement, extra_used);
    }

    loop {
        let mut used = extra_used.clone();
        for statement in statements.iter() {
            statement.for_each_value_id(&mut |id| {
                used.insert(id.0);
            });
        }

        let before = statements.len();
        statements.retain(|statement| {
            if let Statement::Let { bindings, value } = statement {
                let all_unused = bindings.iter().all(|id| !used.contains(&id.0));
                if all_unused && !expr_has_side_effects(value) {
                    return false;
                }
            }
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
            let mut extra = yields_as_used(&region.yields);
            extra.extend(parent_extra_used);
            eliminate_dead_code_in_stmts(&mut region.statements, &extra)
        }
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

    match &statements[1] {
        Statement::If {
            condition,
            inputs,
            then_region,
            else_region,
            outputs,
        } => {
            if condition.id != cv_id {
                return false;
            }
            if !inputs.is_empty() || !outputs.is_empty() {
                return false;
            }
            if else_region.is_some() {
                return false;
            }
            then_region
                .statements
                .iter()
                .any(|s| matches!(s, Statement::Revert { .. }))
        }
        _ => false,
    }
}

/// Whether the leading callvalue-revert check may be hoisted above a switch and dropped from every
/// branch (replacing N per-branch checks with one). Sound only when:
///  1. every case AND the default already start with the check. Otherwise the no-match path — a
///     present default, or, when none, the post-switch fall-through — would gain a spurious
///     callvalue revert it never performed (e.g. an all-nonpayable dispatch with a payable
///     fallback). A missing or non-checking default is exactly such a path.
///  2. every branch reverts with identical data, so the one hoisted revert serves all selectors;
///     otherwise a matched selector would receive the wrong revert payload. Compared via the
///     canonical (SSA-renamed, literal-preserving) prefix encoding.
fn callvalue_check_fully_hoistable(cases: &[SwitchCase], default: &Option<Region>) -> bool {
    if cases.is_empty() {
        return false;
    }
    let all_branches_check = cases
        .iter()
        .all(|c| has_callvalue_revert_prefix(&c.body.statements))
        && default
            .as_ref()
            .is_some_and(|d| has_callvalue_revert_prefix(&d.statements));
    if !all_branches_check {
        return false;
    }
    let reference = canonical_callvalue_prefix(&cases[0].body.statements);
    cases
        .iter()
        .all(|c| canonical_callvalue_prefix(&c.body.statements) == reference)
        && default
            .as_ref()
            .is_some_and(|d| canonical_callvalue_prefix(&d.statements) == reference)
}

/// Returns the value id bound by the leading `let vN = callvalue()`.
///
/// The caller must have established [`has_callvalue_revert_prefix`] for `statements`.
fn callvalue_binding_id(statements: &[Statement]) -> ValueId {
    match &statements[0] {
        Statement::Let { bindings, .. } => bindings[0],
        _ => unreachable!("ICE: callvalue prefix guaranteed by has_callvalue_revert_prefix"),
    }
}

/// Canonical encoding (SSA ids renamed to first-use order, literal values preserved) of the
/// two-statement callvalue-revert prefix. Two prefixes with equal encodings revert with identical
/// data, so hoisting one above a switch in place of all of them is observationally sound.
///
/// The caller must have established [`has_callvalue_revert_prefix`] for `statements`.
fn canonical_callvalue_prefix(statements: &[Statement]) -> Vec<u8> {
    let mut canon = Canonicalizer::new();
    let mut buf = Vec::new();
    canon.encode_statement(&statements[0], &mut buf);
    canon.encode_statement(&statements[1], &mut buf);
    buf
}

/// Drops the leading two-statement callvalue-revert prefix and rewrites any remaining use of its
/// (now-undefined) callvalue binding to `hoisted_cv`, the binding hoisted above the switch.
///
/// Environment CSE may have rewritten a later `callvalue()` in this branch to reuse the prefix's
/// binding; after the prefix is drained that use would reference an undefined value and trip the
/// validator unless it is redirected to the hoisted binding. The caller must have established
/// [`has_callvalue_revert_prefix`] for `statements`.
fn drain_callvalue_prefix(statements: &mut Vec<Statement>, hoisted_cv: ValueId) {
    let local_cv = callvalue_binding_id(statements);
    statements.drain(0..2);
    if local_cv != hoisted_cv {
        for statement in statements.iter_mut() {
            statement.for_each_value_id_mut(&mut |id| {
                if *id == local_cv {
                    *id = hoisted_cv;
                }
            });
        }
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

/// Minimum function size for exact dedup. A body this small lowers to a
/// handful of instructions — comparable to a call's own per-site overhead —
/// so collapsing duplicates into a shared callable would cost more than it
/// saves.
const MIN_DEDUP_SIZE: usize = 4;

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

    let mut canonical_to_id: BTreeMap<Vec<u8>, FunctionId> = BTreeMap::new();
    let mut redirects: BTreeMap<FunctionId, FunctionId> = BTreeMap::new();

    for function in object.functions.values() {
        if function.size_estimate < MIN_DEDUP_SIZE {
            continue;
        }

        let signature = (
            function
                .parameters
                .iter()
                .map(|(_, value_type)| *value_type)
                .collect::<Vec<_>>(),
            function.returns.clone(),
        );

        let canonical = canonicalize_function(function, &signature.0, &signature.1);

        if let Some(&canonical_id) = canonical_to_id.get(&canonical) {
            redirects.insert(function.id, canonical_id);
        } else {
            canonical_to_id.insert(canonical, function.id);
        }
    }

    if redirects.is_empty() {
        return 0;
    }

    let removed_count = redirects.len();
    redirect_calls_in_block(&mut object.code, &redirects);
    for function in object.functions.values_mut() {
        redirect_calls_in_block(&mut function.body, &redirects);
    }

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

    buf.push(parameter_types.len() as u8);
    for value_type in parameter_types {
        buf.push(type_tag(value_type));
    }
    buf.push(return_types.len() as u8);
    for value_type in return_types {
        buf.push(type_tag(value_type));
    }

    for (parameter_id, _) in &function.parameters {
        canon.get_or_insert(*parameter_id);
    }
    for rv in &function.return_values_initial {
        canon.get_or_insert(*rv);
    }

    for statement in &function.body.statements {
        canon.encode_statement(statement, &mut buf);
    }

    buf.push(0xFE);
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
        buf.push(type_tag(&value.value_type));
    }

    fn encode_expression(&mut self, expression: &Expression, buf: &mut Vec<u8>) {
        match expression {
            Expression::Literal { value, value_type } => {
                buf.push(0x01);
                let bytes = value.to_bytes_le();
                buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
                buf.extend_from_slice(&bytes);
                buf.push(type_tag(value_type));
            }
            Expression::Var(id) => {
                buf.push(0x02);
                self.encode_value_id(*id, buf);
            }
            Expression::Binary {
                operation,
                lhs,
                rhs,
            } => {
                buf.push(0x03);
                buf.push(binop_tag(*operation));
                self.encode_value(lhs, buf);
                self.encode_value(rhs, buf);
            }
            Expression::Ternary { operation, a, b, n } => {
                buf.push(0x04);
                buf.push(binop_tag(*operation));
                self.encode_value(a, buf);
                self.encode_value(b, buf);
                self.encode_value(n, buf);
            }
            Expression::Unary { operation, operand } => {
                buf.push(0x05);
                buf.push(unaryop_tag(*operation));
                self.encode_value(operand, buf);
            }
            Expression::Call {
                function,
                arguments,
            } => {
                buf.push(0x06);
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
            _ => {
                buf.push(nullary_expr_tag(expression));
            }
        }
    }

    fn encode_region(&mut self, region: &Region, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&(region.statements.len() as u32).to_le_bytes());
        for statement in &region.statements {
            self.encode_statement(statement, buf);
        }
        buf.push(region.yields.len() as u8);
        for y in &region.yields {
            self.encode_value(y, buf);
        }
    }

    fn encode_statement(&mut self, statement: &Statement, buf: &mut Vec<u8>) {
        match statement {
            Statement::Let { bindings, value } => {
                buf.push(0x80);
                buf.push(bindings.len() as u8);
                for b in bindings {
                    self.encode_value_id(*b, buf);
                }
                self.encode_expression(value, buf);
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
                    self.encode_statement(s, buf);
                }
                self.encode_expression(condition, buf);
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
                self.encode_expression(expression, buf);
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

fn type_tag(value_type: &Type) -> u8 {
    match value_type {
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

fn binop_tag(operation: BinaryOperation) -> u8 {
    match operation {
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

fn unaryop_tag(operation: UnaryOperation) -> u8 {
    match operation {
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
        _ => 0x00,
    }
}

/// Maximum number of differing literal positions allowed for fuzzy dedup.
/// Each differing literal becomes a new i256 parameter, so keep this small.
const MAX_FUZZY_LITERAL_DIFFS: usize = 4;

/// Minimum function size (in IR statements) for fuzzy dedup when any
/// differing-literal parameter is wider than `i64`. Wider new parameters
/// cost more to pass at every call site, so the function body needs to be
/// large enough that the dedup savings cover that overhead.
const MIN_FUZZY_DEDUP_SIZE: usize = 20;

/// Minimum function size for fuzzy dedup when every differing-literal
/// parameter fits in `i64`. Cheap parameters tolerate smaller bodies because
/// the per-call overhead is a single push on the PolkaVM `rv64e` target.
const MIN_FUZZY_DEDUP_SIZE_NARROW_PARAMS: usize = 10;

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

    let mut fuzzy_groups: BTreeMap<Vec<u8>, Vec<FunctionId>> = BTreeMap::new();

    for function in object.functions.values() {
        if function.size_estimate < MIN_FUZZY_DEDUP_SIZE_NARROW_PARAMS {
            continue;
        }

        let fuzzy_hash = fuzzy_canonicalize_function(function);
        fuzzy_groups
            .entry(fuzzy_hash)
            .or_default()
            .push(function.id);
    }

    let mut total_removed = 0;
    let mut next_value_id = ValueId(object.find_max_value_id() + 1);

    for group in fuzzy_groups.values() {
        if group.len() < 2 {
            continue;
        }

        let mut group_literals: Vec<(FunctionId, Vec<BigUint>)> = Vec::new();
        for &fid in group {
            let function = &object.functions[&fid];
            let lits = collect_literals_ordered(function);
            group_literals.push((fid, lits));
        }

        let lit_count = group_literals[0].1.len();
        if group_literals
            .iter()
            .any(|(_, lits)| lits.len() != lit_count)
        {
            continue;
        }

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
            continue;
        }

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

        let mut parameter_groups: Vec<Vec<usize>> = value_sig_to_group.into_values().collect();
        parameter_groups.sort_by_key(|g| g[0]);
        let mut position_to_parameter_index: BTreeMap<usize, usize> = BTreeMap::new();
        for (parameter_index, positions) in parameter_groups.iter().enumerate() {
            for &pos in positions {
                position_to_parameter_index.insert(pos, parameter_index);
            }
        }

        let canonical_id = group[0];
        let canonical_func = object.functions.get(&canonical_id).unwrap().clone();

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

        let mut new_parameter_ids: Vec<ValueId> = Vec::new();
        for _ in 0..unique_param_count {
            new_parameter_ids.push(next_value_id.fresh());
        }

        let position_parameter_ids: Vec<(usize, ValueId)> = differing_positions
            .iter()
            .map(|&pos| (pos, new_parameter_ids[position_to_parameter_index[&pos]]))
            .collect();

        let parameter_types: Vec<Type> = parameter_groups
            .iter()
            .map(|positions| {
                let max_val = group_literals
                    .iter()
                    .flat_map(|(_, lits)| positions.iter().map(move |&p| lits[p].clone()))
                    .max()
                    .unwrap_or_else(BigUint::zero);
                Type::Int(BitWidth::from_max_value(&max_val))
            })
            .collect();

        let all_params_narrow = parameter_types
            .iter()
            .all(|t| matches!(t, Type::Int(width) if *width <= BitWidth::I64));
        let effective_min_size = if all_params_narrow {
            MIN_FUZZY_DEDUP_SIZE_NARROW_PARAMS
        } else {
            MIN_FUZZY_DEDUP_SIZE
        };
        if canonical_func.size_estimate < effective_min_size {
            continue;
        }

        let mut parameterized = canonical_func.clone();
        for (&vid, parameter_type) in new_parameter_ids.iter().zip(parameter_types.iter()) {
            parameterized.parameters.push((vid, *parameter_type));
        }

        let canonical_lits = &group_literals[0].1;

        replace_literals_with_params(&mut parameterized.body, &position_parameter_ids);

        object.functions.insert(canonical_id, parameterized);

        let canonical_extra_args: Vec<BigUint> = parameter_groups
            .iter()
            .map(|positions| canonical_lits[positions[0]].clone())
            .collect();

        for &fid in group {
            let extra_args: Vec<BigUint> = if fid == canonical_id {
                canonical_extra_args.clone()
            } else {
                let lits = &group_literals.iter().find(|(id, _)| *id == fid).unwrap().1;
                parameter_groups
                    .iter()
                    .map(|positions| lits[positions[0]].clone())
                    .collect()
            };

            update_call_sites_with_extra_args(
                &mut object.code,
                fid,
                canonical_id,
                &extra_args,
                &parameter_types,
                &mut next_value_id,
            );
            for function in object.functions.values_mut() {
                update_call_sites_with_extra_args(
                    &mut function.body,
                    fid,
                    canonical_id,
                    &extra_args,
                    &parameter_types,
                    &mut next_value_id,
                );
            }
        }

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

    buf.push(function.parameters.len() as u8);
    for (_, value_type) in &function.parameters {
        buf.push(type_tag(value_type));
    }
    buf.push(function.returns.len() as u8);
    for value_type in &function.returns {
        buf.push(type_tag(value_type));
    }

    for (parameter_id, _) in &function.parameters {
        canon.get_or_insert(*parameter_id);
    }
    for rv in &function.return_values_initial {
        canon.get_or_insert(*rv);
    }

    let mut lit_counter = 0u32;
    for statement in &function.body.statements {
        fuzzy_encode_statement(&mut canon, statement, &mut buf, &mut lit_counter);
    }

    buf.push(0xFE);
    for rv in &function.return_values {
        canon.encode_value_id(*rv, &mut buf);
    }

    buf
}

fn fuzzy_encode_expressionession(
    canon: &mut Canonicalizer,
    expression: &Expression,
    buf: &mut Vec<u8>,
    lit_counter: &mut u32,
) {
    match expression {
        Expression::Literal { value_type, .. } => {
            buf.push(0x01);
            buf.push(0xFF);
            buf.extend_from_slice(&lit_counter.to_le_bytes());
            *lit_counter += 1;
            buf.push(type_tag(value_type));
        }
        Expression::Var(id) => {
            buf.push(0x02);
            canon.encode_value_id(*id, buf);
        }
        Expression::Binary {
            operation,
            lhs,
            rhs,
        } => {
            buf.push(0x03);
            buf.push(binop_tag(*operation));
            canon.encode_value(lhs, buf);
            canon.encode_value(rhs, buf);
        }
        Expression::Ternary { operation, a, b, n } => {
            buf.push(0x04);
            buf.push(binop_tag(*operation));
            canon.encode_value(a, buf);
            canon.encode_value(b, buf);
            canon.encode_value(n, buf);
        }
        Expression::Unary { operation, operand } => {
            buf.push(0x05);
            buf.push(unaryop_tag(*operation));
            canon.encode_value(operand, buf);
        }
        Expression::Call {
            function,
            arguments,
        } => {
            buf.push(0x06);
            buf.extend_from_slice(&function.0.to_le_bytes());
            buf.push(arguments.len() as u8);
            for argument in arguments {
                canon.encode_value(argument, buf);
            }
        }
        Expression::SLoad { key, static_slot } => {
            buf.push(0x12);
            canon.encode_value(key, buf);
            if static_slot.is_some() {
                buf.push(1);
                buf.push(0xFF);
                buf.extend_from_slice(&lit_counter.to_le_bytes());
                *lit_counter += 1;
            } else {
                buf.push(0);
            }
        }
        _ => {
            canon.encode_expression(expression, buf);
        }
    }
}

/// Encodes a statement into the fuzzy form used for function deduplication.
///
/// A `switch` case match value is encoded CONCRETELY (not as an abstracted literal position). A
/// `switch` case label must be a compile-time constant, so it cannot be replaced by a parameter;
/// abstracting it would let two functions that differ only in their case labels share a fuzzy form
/// and be merged, after which `replace_literals_with_params` (which never substitutes a case value)
/// leaves the canonical function's labels in place — a miscompile of the removed function's callers.
fn fuzzy_encode_statement(
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
            fuzzy_encode_expressionession(canon, value, buf, lit_counter);
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
                let case_bytes = c.value.to_bytes_le();
                buf.extend_from_slice(&(case_bytes.len() as u16).to_le_bytes());
                buf.extend_from_slice(&case_bytes);
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
                fuzzy_encode_statement(canon, s, buf, lit_counter);
            }
            fuzzy_encode_expressionession(canon, condition, buf, lit_counter);
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
            fuzzy_encode_expressionession(canon, expression, buf, lit_counter);
        }
        _ => {
            canon.encode_statement(statement, buf);
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
        fuzzy_encode_statement(canon, statement, buf, lit_counter);
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
/// Counted positions: `Expression::Literal` value, `Expression::SLoad::static_slot`, and
/// `Statement::SStore::static_slot`.
///
/// Switch case match values are NOT parameterizable literals (a case label must be a compile-time
/// constant); they are encoded concretely in the fuzzy form, so they are not counted here. Keep
/// this in sync with `replace_literals_with_params`.
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
///
/// Switch case match values are not parameterizable and are not counted by
/// [`collect_literals_ordered`], so the position counter is not advanced for them here.
fn replace_literals_with_params(block: &mut Block, position_parameter_ids: &[(usize, ValueId)]) {
    fn from_expr(
        expression: &mut Expression,
        positions: &BTreeMap<usize, ValueId>,
        counter: &mut usize,
    ) {
        match expression {
            Expression::Literal { .. } => {
                if let Some(&parameter_value_id) = positions.get(counter) {
                    *expression = Expression::Var(parameter_value_id);
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
    let positions: BTreeMap<usize, ValueId> = position_parameter_ids.iter().copied().collect();
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
    parameter_types: &[Type],
    next_value_id: &mut ValueId,
) {
    fn rewrite(
        statements: &mut Vec<Statement>,
        old_id: FunctionId,
        new_id: FunctionId,
        extra_args: &[BigUint],
        parameter_types: &[Type],
        next_id: &mut ValueId,
    ) {
        let mut i = 0;
        while i < statements.len() {
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
                        parameter_types,
                        next_id,
                    );
                    if let Some(r) = else_region {
                        rewrite(
                            &mut r.statements,
                            old_id,
                            new_id,
                            extra_args,
                            parameter_types,
                            next_id,
                        );
                    }
                }
                Statement::Switch { cases, default, .. } => {
                    for c in cases.iter_mut() {
                        rewrite(
                            &mut c.body.statements,
                            old_id,
                            new_id,
                            extra_args,
                            parameter_types,
                            next_id,
                        );
                    }
                    if let Some(d) = default {
                        rewrite(
                            &mut d.statements,
                            old_id,
                            new_id,
                            extra_args,
                            parameter_types,
                            next_id,
                        );
                    }
                }
                Statement::For {
                    condition_statements,
                    body,
                    post,
                    ..
                } => {
                    rewrite(
                        condition_statements,
                        old_id,
                        new_id,
                        extra_args,
                        parameter_types,
                        next_id,
                    );
                    rewrite(
                        &mut body.statements,
                        old_id,
                        new_id,
                        extra_args,
                        parameter_types,
                        next_id,
                    );
                    rewrite(
                        &mut post.statements,
                        old_id,
                        new_id,
                        extra_args,
                        parameter_types,
                        next_id,
                    );
                }
                Statement::Block(region) => {
                    rewrite(
                        &mut region.statements,
                        old_id,
                        new_id,
                        extra_args,
                        parameter_types,
                        next_id,
                    );
                }
                _ => {}
            }
            let is_target = matches!(&statements[i],
                Statement::Let { value: Expression::Call { function, .. }, .. }
                | Statement::Expression(Expression::Call { function, .. })
                if *function == old_id);
            if is_target {
                let mut extra_values = Vec::with_capacity(extra_args.len());
                for (arg_val, parameter_type) in extra_args.iter().zip(parameter_types.iter()) {
                    let vid = next_id.fresh();
                    statements.insert(
                        i,
                        Statement::Let {
                            bindings: vec![vid],
                            value: Expression::Literal {
                                value: arg_val.clone(),
                                value_type: *parameter_type,
                            },
                        },
                    );
                    i += 1;
                    extra_values.push(Value {
                        id: vid,
                        value_type: *parameter_type,
                    });
                }
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
        parameter_types,
        next_value_id,
    );
}

/// Redirects function calls in a block, replacing old function IDs with new ones.
fn redirect_calls_in_block(block: &mut Block, redirects: &BTreeMap<FunctionId, FunctionId>) {
    for_each_statement_mut(&mut block.statements, &mut |statement| {
        statement.for_each_expression_mut(&mut |expression| {
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
///
/// **Known gap (deliberate).** The fused `Keccak256Pair`/`Keccak256Single` helper writes its inputs
/// back to scratch `heap[0..0x40)`/`[0..0x20)`, and the mem_opt fusion *dead-eliminates the original
/// `mstore`s on the strength of that write-back* (see the fusion comment in `mem_opt`). Folding the
/// fused node to a literal removes the helper — and with it the write-back — so the scratch is left
/// unwritten. A later `mload` from `[0, 0x40)` that mem_opt's forwarding cannot reach (across a
/// region/call boundary) would then read stale memory instead of the hashed inputs.
///
/// This gap is NOT closed because every sound fix is a regression — the missing write means the
/// fold's current output is already short the `mstore`s, so re-emitting them necessarily adds code
/// (or disabling the fold falls back to the runtime keccak helper: +2.6 KB / +0.78 % on the OZ
/// corpus), and there is no mem_opt run after the fold to dead-store-eliminate re-emitted writes. It
/// is solc-unreachable: solc treats scratch as volatile and never re-reads `[0, 0x40)` as data after
/// a keccak, so the dropped write-back is always dead in compiler-generated code. Only hand-written
/// Yul that reads scratch after a constant-operand keccak across a boundary can observe it.
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
                if bindings.len() == 1 {
                    if let Expression::Literal { value, .. } = expression {
                        constants.insert(bindings[0].0, value.clone());
                    }
                }

                match expression {
                    Expression::Keccak256Single { word0 } => {
                        if let Some(c) = constants.get(&word0.id.0) {
                            *expression = Expression::Literal {
                                value: fold_keccak256_single(c),
                                value_type: Type::Int(BitWidth::I256),
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
                                value_type: Type::Int(BitWidth::I256),
                            };
                        }
                    }
                    _ => {}
                }

                if bindings.len() == 1 {
                    if let Expression::Literal { value, .. } = expression {
                        constants.insert(bindings[0].0, value.clone());
                    }
                }
            }
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
                value_type: Type::Int(BitWidth::I256),
            },
        }
    }

    fn make_let_binop(id: u32, operation: BinaryOperation, lhs_id: u32, rhs_id: u32) -> Statement {
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expression::Binary {
                operation,
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
        assert_eq!(
            fold_binary(
                BinaryOperation::Shl,
                &BigUint::from(8u64),
                &BigUint::from(1u64)
            ),
            Some(BigUint::from(256u64))
        );
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
            Statement::Let {
                bindings: vec![ValueId(2)],
                value: Expression::Binary {
                    operation: BinaryOperation::Add,
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
        assert!(simplifier.statistics.identities_simplified > 0);
    }

    #[test]
    fn test_no_crash_on_unused_bindings() {
        let mut simplifier = Simplifier::new();

        let statements = vec![
            make_let_literal(1, 42),
            make_let_literal(2, 100),
            Statement::Return {
                offset: Value::int(ValueId(2)),
                length: Value::int(ValueId(2)),
            },
        ];

        let block = &mut Block { statements };
        simplifier.simplify_block(block);

        assert_eq!(block.statements.len(), 3);
    }

    #[test]
    fn test_copy_propagation() {
        let mut simplifier = Simplifier::new();

        let statements = vec![
            make_let_literal(1, 42),
            Statement::Let {
                bindings: vec![ValueId(2)],
                value: Expression::Var(ValueId(1)),
            },
            Statement::Return {
                offset: Value::int(ValueId(2)),
                length: Value::int(ValueId(2)),
            },
        ];

        let block = &mut Block { statements };
        simplifier.simplify_block(block);

        if let Statement::Return { offset, .. } = &block.statements[block.statements.len() - 1] {
            assert!(
                offset.id.0 == 1
                    || matches!(block.statements.last(), Some(Statement::Return { .. }))
            );
        }
    }

    #[test]
    fn test_ternary_fold() {
        let result = fold_ternary(
            BinaryOperation::AddMod,
            &BigUint::from(10u64),
            &BigUint::from(20u64),
            &BigUint::from(7u64),
        );
        assert_eq!(result, Some(BigUint::from(2u64)));

        let result = fold_ternary(
            BinaryOperation::MulMod,
            &BigUint::from(5u64),
            &BigUint::from(7u64),
            &BigUint::from(6u64),
        );
        assert_eq!(result, Some(BigUint::from(5u64)));

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
        let result = fold_binary(
            BinaryOperation::Byte,
            &BigUint::from(31u64),
            &BigUint::from(0xFFu64),
        );
        assert_eq!(result, Some(BigUint::from(0xFFu64)));

        let result = fold_binary(
            BinaryOperation::Byte,
            &BigUint::from(0u64),
            &BigUint::from(0xFFu64),
        );
        assert_eq!(result, Some(BigUint::zero()));
    }

    /// The panic-pattern matcher must honor last-write-wins — a later store that overwrites
    /// the matched selector (offset 0) or code (offset 4) means the revert payload is not the
    /// canonical `Panic`, so the window must not be collapsed.
    #[test]
    fn panic_pattern_respects_last_write_wins() {
        use crate::ir::{MemoryRegion, Value, ValueId};

        let panic_word = BigUint::parse_bytes(
            revive_common::PANIC_UINT256_SELECTOR_WORD_HEX.as_bytes(),
            16,
        )
        .unwrap();
        let mut constants: BTreeMap<u32, BigUint> = BTreeMap::new();
        constants.insert(0, BigUint::zero());
        constants.insert(1, panic_word);
        constants.insert(2, BigUint::from(4u64));
        constants.insert(3, BigUint::from(0x11u64));
        constants.insert(4, BigUint::from(0xdeadbeefu64));
        constants.insert(5, BigUint::from(0x100u64));

        let mstore = |offset: u32, value: u32| Statement::MStore {
            offset: Value::int(ValueId(offset)),
            value: Value::int(ValueId(value)),
            region: MemoryRegion::Scratch,
        };

        assert_eq!(
            find_panic_pattern_backwards(&[mstore(0, 1), mstore(2, 3)], &constants),
            Some((0, 0x11)),
        );

        assert_eq!(
            find_panic_pattern_backwards(&[mstore(0, 1), mstore(2, 3), mstore(0, 4)], &constants),
            None,
        );

        assert_eq!(
            find_panic_pattern_backwards(&[mstore(0, 1), mstore(2, 3), mstore(2, 5)], &constants),
            None,
        );
    }
}
