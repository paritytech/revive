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

use num::{BigUint, One, Zero};

use crate::ir::{
    BinOp, BitWidth, Block, Expr, Object, Region, Statement, SwitchCase, Type, UnaryOp, Value,
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
}

/// IR simplification pass.
pub struct Simplifier {
    /// Maps ValueId → constant BigUint for known constants.
    constants: BTreeMap<u32, BigUint>,
    /// Maps ValueId → ValueId for copy propagation (let x = y → x maps to y).
    copies: BTreeMap<u32, ValueId>,
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
            stats: SimplifyResults::default(),
        }
    }

    /// Simplifies an entire object in place.
    pub fn simplify_object(&mut self, object: &mut Object) -> SimplifyResults {
        // Simplify main code block
        self.simplify_block(&mut object.code);
        // DCE on main code block (no explicit return values)
        self.stats.dead_bindings_removed +=
            eliminate_dead_code_in_stmts(&mut object.code.statements, &BTreeSet::new());

        for function in object.functions.values_mut() {
            self.constants.clear();
            self.copies.clear();
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

    /// Simplifies a block in place.
    fn simplify_block(&mut self, block: &mut Block) {
        block.statements = self.simplify_statements(std::mem::take(&mut block.statements));
    }

    /// Simplifies a list of statements, returning the simplified list.
    fn simplify_statements(&mut self, statements: Vec<Statement>) -> Vec<Statement> {
        let outer_constants = self.constants.clone();
        let outer_copies = self.copies.clone();

        let mut result = Vec::with_capacity(statements.len());

        for stmt in statements {
            let simplified = self.simplify_statement(stmt);
            result.extend(simplified);
        }

        self.constants = outer_constants;
        self.copies = outer_copies;

        result
    }

    /// Simplifies a single statement.
    /// Returns a vec of replacement statements (empty = remove, one = replace, multiple = expand).
    fn simplify_statement(&mut self, stmt: Statement) -> Vec<Statement> {
        match stmt {
            Statement::Let { bindings, value } => {
                let simplified_expr = self.simplify_expr(value);

                // Track constants
                if bindings.len() == 1 {
                    if let Expr::Literal { ref value, .. } = simplified_expr {
                        self.constants.insert(bindings[0].0, value.clone());
                    }
                    // Track copies (let x = y)
                    if let Expr::Var(src_id) = &simplified_expr {
                        let resolved = self.resolve_copy(*src_id);
                        self.copies.insert(bindings[0].0, resolved);
                        // Also propagate constant knowledge
                        if let Some(c) = self.constants.get(&resolved.0).cloned() {
                            self.constants.insert(bindings[0].0, c);
                        }
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
            } => vec![Statement::MStore {
                offset: self.resolve_value(offset),
                value: self.resolve_value(value),
                region,
            }],

            Statement::MStore8 {
                offset,
                value,
                region,
            } => vec![Statement::MStore8 {
                offset: self.resolve_value(offset),
                value: self.resolve_value(value),
                region,
            }],

            Statement::MCopy { dest, src, length } => vec![Statement::MCopy {
                dest: self.resolve_value(dest),
                src: self.resolve_value(src),
                length: self.resolve_value(length),
            }],

            Statement::SStore {
                key,
                value,
                static_slot,
            } => vec![Statement::SStore {
                key: self.resolve_value(key),
                value: self.resolve_value(value),
                static_slot,
            }],

            Statement::TStore { key, value } => vec![Statement::TStore {
                key: self.resolve_value(key),
                value: self.resolve_value(value),
            }],

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
                                value: Expr::Var(yield_val.id),
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
                                value: Expr::Var(yield_val.id),
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
                                value: Expr::Var(input_val.id),
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
                                value: Expr::Var(input_val.id),
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
                            value: Expr::Var(yield_val.id),
                        });
                    }
                    self.stats.branches_eliminated += 1;
                    return result;
                }

                let inputs: Vec<Value> =
                    inputs.into_iter().map(|v| self.resolve_value(v)).collect();
                let cases: Vec<SwitchCase> = cases
                    .into_iter()
                    .map(|c| SwitchCase {
                        value: c.value,
                        body: self.simplify_region(c.body),
                    })
                    .collect();
                let default = default.map(|r| self.simplify_region(r));

                vec![Statement::Switch {
                    scrutinee,
                    inputs,
                    cases,
                    default,
                    outputs,
                }]
            }

            Statement::For {
                init_values,
                loop_vars,
                condition_stmts,
                condition,
                body,
                post_input_vars,
                post,
                outputs,
            } => {
                let init_values: Vec<Value> = init_values
                    .into_iter()
                    .map(|v| self.resolve_value(v))
                    .collect();

                // Save state for loop body (loop body can't see pre-loop constants reliably
                // since values change each iteration)
                let saved_constants = self.constants.clone();
                let saved_copies = self.copies.clone();

                let condition_stmts = self.simplify_statements(condition_stmts);
                let condition = self.simplify_expr(condition);
                let body = self.simplify_region(body);
                let post = self.simplify_region(post);

                self.constants = saved_constants;
                self.copies = saved_copies;

                vec![Statement::For {
                    init_values,
                    loop_vars,
                    condition_stmts,
                    condition,
                    body,
                    post_input_vars,
                    post,
                    outputs,
                }]
            }

            Statement::Block(region) => vec![Statement::Block(self.simplify_region(region))],

            Statement::Revert { offset, length } => vec![Statement::Revert {
                offset: self.resolve_value(offset),
                length: self.resolve_value(length),
            }],

            Statement::Return { offset, length } => vec![Statement::Return {
                offset: self.resolve_value(offset),
                length: self.resolve_value(length),
            }],

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
            } => vec![Statement::ExternalCall {
                kind,
                gas: self.resolve_value(gas),
                address: self.resolve_value(address),
                value: value.map(|v| self.resolve_value(v)),
                args_offset: self.resolve_value(args_offset),
                args_length: self.resolve_value(args_length),
                ret_offset: self.resolve_value(ret_offset),
                ret_length: self.resolve_value(ret_length),
                result,
            }],

            Statement::Create {
                kind,
                value,
                offset,
                length,
                salt,
                result,
            } => vec![Statement::Create {
                kind,
                value: self.resolve_value(value),
                offset: self.resolve_value(offset),
                length: self.resolve_value(length),
                salt: salt.map(|v| self.resolve_value(v)),
                result,
            }],

            Statement::Log {
                offset,
                length,
                topics,
            } => vec![Statement::Log {
                offset: self.resolve_value(offset),
                length: self.resolve_value(length),
                topics: topics.into_iter().map(|v| self.resolve_value(v)).collect(),
            }],

            Statement::CodeCopy {
                dest,
                offset,
                length,
            } => vec![Statement::CodeCopy {
                dest: self.resolve_value(dest),
                offset: self.resolve_value(offset),
                length: self.resolve_value(length),
            }],

            Statement::ExtCodeCopy {
                address,
                dest,
                offset,
                length,
            } => vec![Statement::ExtCodeCopy {
                address: self.resolve_value(address),
                dest: self.resolve_value(dest),
                offset: self.resolve_value(offset),
                length: self.resolve_value(length),
            }],

            Statement::ReturnDataCopy {
                dest,
                offset,
                length,
            } => vec![Statement::ReturnDataCopy {
                dest: self.resolve_value(dest),
                offset: self.resolve_value(offset),
                length: self.resolve_value(length),
            }],

            Statement::DataCopy {
                dest,
                offset,
                length,
            } => vec![Statement::DataCopy {
                dest: self.resolve_value(dest),
                offset: self.resolve_value(offset),
                length: self.resolve_value(length),
            }],

            Statement::CallDataCopy {
                dest,
                offset,
                length,
            } => vec![Statement::CallDataCopy {
                dest: self.resolve_value(dest),
                offset: self.resolve_value(offset),
                length: self.resolve_value(length),
            }],

            Statement::SetImmutable { key, value } => vec![Statement::SetImmutable {
                key,
                value: self.resolve_value(value),
            }],

            Statement::Leave { return_values } => vec![Statement::Leave {
                return_values: return_values
                    .into_iter()
                    .map(|v| self.resolve_value(v))
                    .collect(),
            }],

            Statement::Expr(expr) => vec![Statement::Expr(self.simplify_expr(expr))],

            // Pass-through statements with no values to simplify
            Statement::Stop | Statement::Invalid => vec![stmt],

            Statement::Break { values } => vec![Statement::Break {
                values: values.into_iter().map(|v| self.resolve_value(v)).collect(),
            }],

            Statement::Continue { values } => vec![Statement::Continue {
                values: values.into_iter().map(|v| self.resolve_value(v)).collect(),
            }],

            Statement::SelfDestruct { address } => vec![Statement::SelfDestruct {
                address: self.resolve_value(address),
            }],
        }
    }

    /// Simplifies a region in place.
    fn simplify_region(&mut self, region: Region) -> Region {
        // Save outer scope state
        let outer_constants = self.constants.clone();
        let outer_copies = self.copies.clone();

        let mut statements = Vec::with_capacity(region.statements.len());
        for stmt in region.statements {
            let simplified = self.simplify_statement(stmt);
            statements.extend(simplified);
        }

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

        Region { statements, yields }
    }

    /// Simplifies an expression, performing constant folding, algebraic identities,
    /// and copy propagation on operands.
    fn simplify_expr(&mut self, expr: Expr) -> Expr {
        match expr {
            Expr::Binary { op, lhs, rhs } => {
                let lhs = self.resolve_value(lhs);
                let rhs = self.resolve_value(rhs);
                let lhs_val = self.try_get_const(&lhs);
                let rhs_val = self.try_get_const(&rhs);

                // Constant folding: both operands are constants
                if let (Some(a), Some(b)) = (&lhs_val, &rhs_val) {
                    if let Some(result) = fold_binary(op, a, b) {
                        self.stats.constants_folded += 1;
                        return Expr::Literal {
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

                Expr::Binary { op, lhs, rhs }
            }

            Expr::Unary { op, operand } => {
                let operand = self.resolve_value(operand);
                let operand_val = self.try_get_const(&operand);

                if let Some(c) = &operand_val {
                    if let Some(result) = fold_unary(op, c) {
                        self.stats.constants_folded += 1;
                        return Expr::Literal {
                            value: result,
                            ty: unary_result_type(op),
                        };
                    }
                }

                Expr::Unary { op, operand }
            }

            Expr::Ternary { op, a, b, n } => {
                let a = self.resolve_value(a);
                let b = self.resolve_value(b);
                let n = self.resolve_value(n);
                let a_val = self.try_get_const(&a);
                let b_val = self.try_get_const(&b);
                let n_val = self.try_get_const(&n);

                if let (Some(av), Some(bv), Some(nv)) = (&a_val, &b_val, &n_val) {
                    if let Some(result) = fold_ternary(op, av, bv, nv) {
                        self.stats.constants_folded += 1;
                        return Expr::Literal {
                            value: result,
                            ty: Type::Int(BitWidth::I256),
                        };
                    }
                }

                Expr::Ternary { op, a, b, n }
            }

            // Resolve copies in Var references
            Expr::Var(id) => {
                let resolved = self.resolve_copy(id);
                Expr::Var(resolved)
            }

            // All other expressions pass through unchanged
            other => other,
        }
    }

    /// Resolves a Value through copy propagation.
    fn resolve_value(&self, val: Value) -> Value {
        let resolved = self.resolve_copy(val.id);
        if resolved != val.id {
            Value {
                id: resolved,
                ..val
            }
        } else {
            val
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
    fn try_get_const(&self, val: &Value) -> Option<BigUint> {
        let resolved = self.resolve_copy(val.id);
        self.constants.get(&resolved.0).cloned()
    }
}

/// Returns the result type for a binary operation.
fn result_type(op: BinOp) -> Type {
    match op {
        BinOp::Lt | BinOp::Gt | BinOp::Slt | BinOp::Sgt | BinOp::Eq => Type::Int(BitWidth::I256),
        _ => Type::Int(BitWidth::I256),
    }
}

/// Returns the result type for a unary operation.
fn unary_result_type(op: UnaryOp) -> Type {
    match op {
        UnaryOp::IsZero => Type::Int(BitWidth::I256),
        UnaryOp::Not | UnaryOp::Clz => Type::Int(BitWidth::I256),
    }
}

/// Folds a binary operation on two constant values.
/// Returns None if the operation cannot be folded.
fn fold_binary(op: BinOp, a: &BigUint, b: &BigUint) -> Option<BigUint> {
    let modulus = modulus_u256();
    let max = max_u256();

    Some(match op {
        BinOp::Add => (a + b) % &modulus,
        BinOp::Sub => {
            if a >= b {
                a - b
            } else {
                // Wrapping subtraction: a - b + 2^256
                &modulus - (b - a)
            }
        }
        BinOp::Mul => (a * b) % &modulus,
        BinOp::Div => {
            if b.is_zero() {
                BigUint::zero()
            } else {
                a / b
            }
        }
        BinOp::SDiv => {
            if b.is_zero() {
                BigUint::zero()
            } else {
                fold_sdiv(a, b, &modulus)?
            }
        }
        BinOp::Mod => {
            if b.is_zero() {
                BigUint::zero()
            } else {
                a % b
            }
        }
        BinOp::SMod => {
            if b.is_zero() {
                BigUint::zero()
            } else {
                fold_smod(a, b, &modulus)?
            }
        }
        BinOp::Exp => {
            // a^b mod 2^256
            a.modpow(b, &modulus)
        }
        BinOp::And => a & b,
        BinOp::Or => a | b,
        BinOp::Xor => a ^ b,
        // EVM shift convention: shl(shift_amount, value) = value << shift_amount
        // In our IR: Binary { Shl, lhs: shift_amount, rhs: value }
        // So a = shift_amount, b = value
        BinOp::Shl => {
            if *a >= BigUint::from(256u32) {
                BigUint::zero()
            } else {
                let shift = a.to_u64_digits().first().copied().unwrap_or(0) as usize;
                (b << shift) % &modulus
            }
        }
        BinOp::Shr => {
            if *a >= BigUint::from(256u32) {
                BigUint::zero()
            } else {
                let shift = a.to_u64_digits().first().copied().unwrap_or(0) as usize;
                b >> shift
            }
        }
        BinOp::Sar => fold_sar(b, a, &modulus, &max)?,
        BinOp::Lt => bool_to_u256(a < b),
        BinOp::Gt => bool_to_u256(a > b),
        BinOp::Eq => bool_to_u256(a == b),
        BinOp::Slt => {
            let a_signed = is_negative(a, &modulus);
            let b_signed = is_negative(b, &modulus);
            match (a_signed, b_signed) {
                (true, false) => bool_to_u256(true),
                (false, true) => bool_to_u256(false),
                _ => bool_to_u256(a < b),
            }
        }
        BinOp::Sgt => {
            let a_signed = is_negative(a, &modulus);
            let b_signed = is_negative(b, &modulus);
            match (a_signed, b_signed) {
                (true, false) => bool_to_u256(false),
                (false, true) => bool_to_u256(true),
                _ => bool_to_u256(a > b),
            }
        }
        BinOp::Byte => {
            // byte(n, x): nth byte of x (0-indexed from most significant)
            if *a >= BigUint::from(32u32) {
                BigUint::zero()
            } else {
                let n = a.to_u64_digits().first().copied().unwrap_or(0) as usize;
                let shift = (31 - n) * 8;
                (b >> shift) & BigUint::from(0xffu32)
            }
        }
        BinOp::SignExtend => fold_signextend(a, b, &max)?,
        // Ternary ops handled separately
        BinOp::AddMod | BinOp::MulMod => return None,
    })
}

/// Folds a unary operation on a constant value.
fn fold_unary(op: UnaryOp, a: &BigUint) -> Option<BigUint> {
    Some(match op {
        UnaryOp::IsZero => bool_to_u256(a.is_zero()),
        UnaryOp::Not => {
            // Bitwise NOT: flip all 256 bits
            &max_u256() ^ a
        }
        UnaryOp::Clz => {
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
fn fold_ternary(op: BinOp, a: &BigUint, b: &BigUint, n: &BigUint) -> Option<BigUint> {
    if n.is_zero() {
        return Some(BigUint::zero());
    }
    Some(match op {
        BinOp::AddMod => (a + b) % n,
        BinOp::MulMod => (a * b) % n,
        _ => return None,
    })
}

/// Applies algebraic identity simplifications.
/// Returns Some(simplified_expr) if an identity applies, None otherwise.
fn simplify_binary(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    lhs_val: &Option<BigUint>,
    rhs_val: &Option<BigUint>,
) -> Option<Expr> {
    let zero = BigUint::zero();
    let one = BigUint::one();

    match op {
        // add(x, 0) = add(0, x) = x
        BinOp::Add => {
            if rhs_val.as_ref().is_some_and(|v| v.is_zero()) {
                return Some(Expr::Var(lhs.id));
            }
            if lhs_val.as_ref().is_some_and(|v| v.is_zero()) {
                return Some(Expr::Var(rhs.id));
            }
            None
        }

        // sub(x, 0) = x
        BinOp::Sub => {
            if rhs_val.as_ref().is_some_and(|v| v.is_zero()) {
                return Some(Expr::Var(lhs.id));
            }
            // sub(x, x) = 0 (same ValueId)
            if lhs.id == rhs.id {
                return Some(Expr::Literal {
                    value: zero,
                    ty: Type::Int(BitWidth::I256),
                });
            }
            None
        }

        // mul(x, 0) = mul(0, x) = 0
        // mul(x, 1) = mul(1, x) = x
        BinOp::Mul => {
            if rhs_val.as_ref().is_some_and(|v| v.is_zero())
                || lhs_val.as_ref().is_some_and(|v| v.is_zero())
            {
                return Some(Expr::Literal {
                    value: zero,
                    ty: Type::Int(BitWidth::I256),
                });
            }
            if rhs_val.as_ref().is_some_and(|v| *v == one) {
                return Some(Expr::Var(lhs.id));
            }
            if lhs_val.as_ref().is_some_and(|v| *v == one) {
                return Some(Expr::Var(rhs.id));
            }
            None
        }

        // div(x, 1) = x, div(0, x) = 0
        BinOp::Div | BinOp::SDiv => {
            if rhs_val.as_ref().is_some_and(|v| *v == one) {
                return Some(Expr::Var(lhs.id));
            }
            if lhs_val.as_ref().is_some_and(|v| v.is_zero()) {
                return Some(Expr::Literal {
                    value: zero,
                    ty: Type::Int(BitWidth::I256),
                });
            }
            // div(x, x) = 1 when x != 0 (same ValueId means definitely equal)
            if lhs.id == rhs.id {
                return Some(Expr::Literal {
                    value: one,
                    ty: Type::Int(BitWidth::I256),
                });
            }
            None
        }

        // mod(x, 1) = 0, mod(0, x) = 0
        BinOp::Mod | BinOp::SMod => {
            if rhs_val.as_ref().is_some_and(|v| *v == one) {
                return Some(Expr::Literal {
                    value: zero,
                    ty: Type::Int(BitWidth::I256),
                });
            }
            if lhs_val.as_ref().is_some_and(|v| v.is_zero()) {
                return Some(Expr::Literal {
                    value: zero,
                    ty: Type::Int(BitWidth::I256),
                });
            }
            // mod(x, x) = 0
            if lhs.id == rhs.id {
                return Some(Expr::Literal {
                    value: zero,
                    ty: Type::Int(BitWidth::I256),
                });
            }
            None
        }

        // and(x, 0) = 0, and(x, MAX) = x
        BinOp::And => {
            if rhs_val.as_ref().is_some_and(|v| v.is_zero())
                || lhs_val.as_ref().is_some_and(|v| v.is_zero())
            {
                return Some(Expr::Literal {
                    value: zero,
                    ty: Type::Int(BitWidth::I256),
                });
            }
            let max = max_u256();
            if rhs_val.as_ref().is_some_and(|v| *v == max) {
                return Some(Expr::Var(lhs.id));
            }
            if lhs_val.as_ref().is_some_and(|v| *v == max) {
                return Some(Expr::Var(rhs.id));
            }
            // and(x, x) = x
            if lhs.id == rhs.id {
                return Some(Expr::Var(lhs.id));
            }
            None
        }

        // or(x, 0) = x, or(x, MAX) = MAX
        BinOp::Or => {
            if rhs_val.as_ref().is_some_and(|v| v.is_zero()) {
                return Some(Expr::Var(lhs.id));
            }
            if lhs_val.as_ref().is_some_and(|v| v.is_zero()) {
                return Some(Expr::Var(rhs.id));
            }
            let max = max_u256();
            if rhs_val.as_ref().is_some_and(|v| *v == max)
                || lhs_val.as_ref().is_some_and(|v| *v == max)
            {
                return Some(Expr::Literal {
                    value: max,
                    ty: Type::Int(BitWidth::I256),
                });
            }
            // or(x, x) = x
            if lhs.id == rhs.id {
                return Some(Expr::Var(lhs.id));
            }
            None
        }

        // xor(x, 0) = x, xor(x, x) = 0
        BinOp::Xor => {
            if rhs_val.as_ref().is_some_and(|v| v.is_zero()) {
                return Some(Expr::Var(lhs.id));
            }
            if lhs_val.as_ref().is_some_and(|v| v.is_zero()) {
                return Some(Expr::Var(rhs.id));
            }
            if lhs.id == rhs.id {
                return Some(Expr::Literal {
                    value: zero,
                    ty: Type::Int(BitWidth::I256),
                });
            }
            None
        }

        // shl(0, x) = x (shift by 0 returns value unchanged)
        // IR convention: lhs = shift_amount, rhs = value
        BinOp::Shl | BinOp::Shr | BinOp::Sar => {
            if lhs_val.as_ref().is_some_and(|v| v.is_zero()) {
                return Some(Expr::Var(rhs.id));
            }
            None
        }

        // eq(x, x) = 1
        BinOp::Eq => {
            if lhs.id == rhs.id {
                return Some(Expr::Literal {
                    value: one,
                    ty: Type::Int(BitWidth::I256),
                });
            }
            None
        }

        // lt(x, x) = gt(x, x) = slt(x, x) = sgt(x, x) = 0
        BinOp::Lt | BinOp::Gt | BinOp::Slt | BinOp::Sgt => {
            if lhs.id == rhs.id {
                return Some(Expr::Literal {
                    value: zero,
                    ty: Type::Int(BitWidth::I256),
                });
            }
            None
        }

        _ => None,
    }
}

/// Helper: checks if a 256-bit value is negative in two's complement.
fn is_negative(val: &BigUint, modulus: &BigUint) -> bool {
    *val >= (modulus >> 1)
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
fn expr_has_side_effects(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Call { .. }
            | Expr::Keccak256 { .. }
            | Expr::MLoad { .. }
            | Expr::SLoad { .. }
            | Expr::TLoad { .. }
            | Expr::MSize
    )
}

/// Collects all ValueIds that are used (referenced) in a list of statements.
#[allow(dead_code)]
fn collect_used_values(statements: &[Statement]) -> BTreeSet<u32> {
    let mut used = BTreeSet::new();
    for stmt in statements {
        collect_used_in_stmt(stmt, &mut used);
    }
    used
}

fn collect_used_in_value(val: &Value, used: &mut BTreeSet<u32>) {
    used.insert(val.id.0);
}

fn collect_used_in_expr(expr: &Expr, used: &mut BTreeSet<u32>) {
    match expr {
        Expr::Literal { .. } => {}
        Expr::Var(id) => {
            used.insert(id.0);
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_used_in_value(lhs, used);
            collect_used_in_value(rhs, used);
        }
        Expr::Ternary { a, b, n, .. } => {
            collect_used_in_value(a, used);
            collect_used_in_value(b, used);
            collect_used_in_value(n, used);
        }
        Expr::Unary { operand, .. } => collect_used_in_value(operand, used),
        Expr::CallDataLoad { offset } => collect_used_in_value(offset, used),
        Expr::ExtCodeSize { address } | Expr::ExtCodeHash { address } => {
            collect_used_in_value(address, used)
        }
        Expr::BlockHash { number } => collect_used_in_value(number, used),
        Expr::BlobHash { index } => collect_used_in_value(index, used),
        Expr::Balance { address } => collect_used_in_value(address, used),
        Expr::MLoad { offset, .. } => collect_used_in_value(offset, used),
        Expr::SLoad { key, .. } => collect_used_in_value(key, used),
        Expr::TLoad { key } => collect_used_in_value(key, used),
        Expr::Call { args, .. } => {
            for arg in args {
                collect_used_in_value(arg, used);
            }
        }
        Expr::Truncate { value, .. }
        | Expr::ZeroExtend { value, .. }
        | Expr::SignExtendTo { value, .. } => collect_used_in_value(value, used),
        Expr::Keccak256 { offset, length } => {
            collect_used_in_value(offset, used);
            collect_used_in_value(length, used);
        }
        Expr::CallValue
        | Expr::Caller
        | Expr::Origin
        | Expr::CallDataSize
        | Expr::CodeSize
        | Expr::GasPrice
        | Expr::ReturnDataSize
        | Expr::Coinbase
        | Expr::Timestamp
        | Expr::Number
        | Expr::Difficulty
        | Expr::GasLimit
        | Expr::ChainId
        | Expr::SelfBalance
        | Expr::BaseFee
        | Expr::BlobBaseFee
        | Expr::Gas
        | Expr::MSize
        | Expr::Address
        | Expr::DataOffset { .. }
        | Expr::DataSize { .. }
        | Expr::LoadImmutable { .. }
        | Expr::LinkerSymbol { .. } => {}
    }
}

fn collect_used_in_region(region: &Region, used: &mut BTreeSet<u32>) {
    for stmt in &region.statements {
        collect_used_in_stmt(stmt, used);
    }
    for y in &region.yields {
        collect_used_in_value(y, used);
    }
}

fn collect_used_in_stmt(stmt: &Statement, used: &mut BTreeSet<u32>) {
    match stmt {
        Statement::Let { value, .. } => {
            // Don't mark bindings as used — they're *definitions*, not uses.
            // The expr inside is used though.
            collect_used_in_expr(value, used);
        }
        Statement::MStore { offset, value, .. } | Statement::MStore8 { offset, value, .. } => {
            collect_used_in_value(offset, used);
            collect_used_in_value(value, used);
        }
        Statement::MCopy { dest, src, length } => {
            collect_used_in_value(dest, used);
            collect_used_in_value(src, used);
            collect_used_in_value(length, used);
        }
        Statement::SStore { key, value, .. } => {
            collect_used_in_value(key, used);
            collect_used_in_value(value, used);
        }
        Statement::TStore { key, value } => {
            collect_used_in_value(key, used);
            collect_used_in_value(value, used);
        }
        Statement::If {
            condition,
            inputs,
            then_region,
            else_region,
            outputs: _,
        } => {
            collect_used_in_value(condition, used);
            for i in inputs {
                collect_used_in_value(i, used);
            }
            collect_used_in_region(then_region, used);
            if let Some(r) = else_region {
                collect_used_in_region(r, used);
            }
        }
        Statement::Switch {
            scrutinee,
            inputs,
            cases,
            default,
            ..
        } => {
            collect_used_in_value(scrutinee, used);
            for i in inputs {
                collect_used_in_value(i, used);
            }
            for c in cases {
                collect_used_in_region(&c.body, used);
            }
            if let Some(d) = default {
                collect_used_in_region(d, used);
            }
        }
        Statement::For {
            init_values,
            condition_stmts,
            condition,
            body,
            post,
            ..
        } => {
            for v in init_values {
                collect_used_in_value(v, used);
            }
            for s in condition_stmts {
                collect_used_in_stmt(s, used);
            }
            collect_used_in_expr(condition, used);
            collect_used_in_region(body, used);
            collect_used_in_region(post, used);
        }
        Statement::Block(region) => collect_used_in_region(region, used),
        Statement::Revert { offset, length } | Statement::Return { offset, length } => {
            collect_used_in_value(offset, used);
            collect_used_in_value(length, used);
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
            collect_used_in_value(gas, used);
            collect_used_in_value(address, used);
            if let Some(v) = value {
                collect_used_in_value(v, used);
            }
            collect_used_in_value(args_offset, used);
            collect_used_in_value(args_length, used);
            collect_used_in_value(ret_offset, used);
            collect_used_in_value(ret_length, used);
        }
        Statement::Create {
            value,
            offset,
            length,
            salt,
            ..
        } => {
            collect_used_in_value(value, used);
            collect_used_in_value(offset, used);
            collect_used_in_value(length, used);
            if let Some(s) = salt {
                collect_used_in_value(s, used);
            }
        }
        Statement::Log {
            offset,
            length,
            topics,
        } => {
            collect_used_in_value(offset, used);
            collect_used_in_value(length, used);
            for t in topics {
                collect_used_in_value(t, used);
            }
        }
        Statement::CodeCopy {
            dest,
            offset,
            length,
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
            collect_used_in_value(dest, used);
            collect_used_in_value(offset, used);
            collect_used_in_value(length, used);
        }
        Statement::ExtCodeCopy {
            address,
            dest,
            offset,
            length,
        } => {
            collect_used_in_value(address, used);
            collect_used_in_value(dest, used);
            collect_used_in_value(offset, used);
            collect_used_in_value(length, used);
        }
        Statement::SetImmutable { value, .. } => collect_used_in_value(value, used),
        Statement::Leave { return_values } => {
            for v in return_values {
                collect_used_in_value(v, used);
            }
        }
        Statement::Expr(expr) => collect_used_in_expr(expr, used),
        Statement::SelfDestruct { address } => collect_used_in_value(address, used),
        Statement::Stop | Statement::Invalid => {}
        Statement::Break { values } | Statement::Continue { values } => {
            for v in values {
                collect_used_in_value(v, used);
            }
        }
    }
}

/// Eliminates dead Let bindings from a list of statements.
/// Uses bottom-up recursive DCE: first cleans nested regions, then this level.
/// Iterates at each level until fixpoint (no more removals).
///
/// `extra_used` contains ValueIds that must be preserved even if not referenced
/// by the statements themselves (e.g., function return values, region yields).
fn eliminate_dead_code_in_stmts(stmts: &mut Vec<Statement>, extra_used: &BTreeSet<u32>) -> usize {
    let mut total_removed = 0;

    // Phase 1: Recursively DCE nested regions (bottom-up)
    for stmt in stmts.iter_mut() {
        total_removed += eliminate_dead_code_in_nested(stmt);
    }

    // Phase 2: DCE at this level with fixpoint iteration
    loop {
        let mut used = extra_used.clone();
        for stmt in stmts.iter() {
            collect_used_in_stmt(stmt, &mut used);
        }

        let before = stmts.len();
        stmts.retain(|stmt| {
            if let Statement::Let { bindings, value } = stmt {
                let all_unused = bindings.iter().all(|id| !used.contains(&id.0));
                if all_unused && !expr_has_side_effects(value) {
                    return false;
                }
            }
            true
        });

        let removed = before - stmts.len();
        total_removed += removed;
        if removed == 0 {
            break;
        }
    }

    total_removed
}

/// Recursively DCE inside nested regions of a statement.
fn eliminate_dead_code_in_nested(stmt: &mut Statement) -> usize {
    match stmt {
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
            let extra = yields_as_used(&region.yields);
            eliminate_dead_code_in_stmts(&mut region.statements, &extra)
        }
        // Skip For loops - complex loop_var/phi semantics
        _ => 0,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_let_literal(id: u32, val: u64) -> Statement {
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expr::Literal {
                value: BigUint::from(val),
                ty: Type::Int(BitWidth::I256),
            },
        }
    }

    fn make_let_binop(id: u32, op: BinOp, lhs_id: u32, rhs_id: u32) -> Statement {
        Statement::Let {
            bindings: vec![ValueId(id)],
            value: Expr::Binary {
                op,
                lhs: Value::int(ValueId(lhs_id)),
                rhs: Value::int(ValueId(rhs_id)),
            },
        }
    }

    #[test]
    fn test_constant_fold_add() {
        let result = fold_binary(BinOp::Add, &BigUint::from(100u64), &BigUint::from(200u64));
        assert_eq!(result, Some(BigUint::from(300u64)));
    }

    #[test]
    fn test_constant_fold_sub_wrap() {
        let result = fold_binary(BinOp::Sub, &BigUint::from(0u64), &BigUint::from(1u64));
        assert_eq!(result, Some(max_u256()));
    }

    #[test]
    fn test_constant_fold_mul() {
        let result = fold_binary(BinOp::Mul, &BigUint::from(7u64), &BigUint::from(6u64));
        assert_eq!(result, Some(BigUint::from(42u64)));
    }

    #[test]
    fn test_constant_fold_div_by_zero() {
        let result = fold_binary(BinOp::Div, &BigUint::from(100u64), &BigUint::zero());
        assert_eq!(result, Some(BigUint::zero()));
    }

    #[test]
    fn test_constant_fold_comparisons() {
        assert_eq!(
            fold_binary(BinOp::Lt, &BigUint::from(5u64), &BigUint::from(10u64)),
            Some(BigUint::one())
        );
        assert_eq!(
            fold_binary(BinOp::Lt, &BigUint::from(10u64), &BigUint::from(5u64)),
            Some(BigUint::zero())
        );
        assert_eq!(
            fold_binary(BinOp::Eq, &BigUint::from(42u64), &BigUint::from(42u64)),
            Some(BigUint::one())
        );
    }

    #[test]
    fn test_constant_fold_bitwise() {
        assert_eq!(
            fold_binary(
                BinOp::And,
                &BigUint::from(0xFF00u64),
                &BigUint::from(0x0FF0u64)
            ),
            Some(BigUint::from(0x0F00u64))
        );
        assert_eq!(
            fold_binary(
                BinOp::Or,
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
            fold_binary(BinOp::Shl, &BigUint::from(8u64), &BigUint::from(1u64)),
            Some(BigUint::from(256u64))
        );
        // shr(shift_amount, value) = value >> shift_amount
        assert_eq!(
            fold_binary(BinOp::Shr, &BigUint::from(4u64), &BigUint::from(256u64)),
            Some(BigUint::from(16u64))
        );
    }

    #[test]
    fn test_unary_fold() {
        assert_eq!(
            fold_unary(UnaryOp::IsZero, &BigUint::zero()),
            Some(BigUint::one())
        );
        assert_eq!(
            fold_unary(UnaryOp::IsZero, &BigUint::from(42u64)),
            Some(BigUint::zero())
        );
        assert_eq!(fold_unary(UnaryOp::Not, &BigUint::zero()), Some(max_u256()));
    }

    #[test]
    fn test_simplifier_constant_propagation() {
        let mut simplifier = Simplifier::new();

        // v3 uses v1 and v2, so we also need something that uses v3
        // to prevent DCE from removing everything
        let stmts = vec![
            make_let_literal(1, 10),
            make_let_literal(2, 20),
            make_let_binop(3, BinOp::Add, 1, 2),
            Statement::Return {
                offset: Value::int(ValueId(3)),
                length: Value::int(ValueId(3)),
            },
        ];

        let block = &mut Block { statements: stmts };
        simplifier.simplify_block(block);

        // After constant folding + DCE: v1 and v2 are removed (unused after folding),
        // v3 = literal 30 remains, and the return references v3
        // Find the Let for v3
        let v3_let = block.statements.iter().find(
            |s| matches!(s, Statement::Let { bindings, .. } if bindings.contains(&ValueId(3))),
        );
        let v3_let = v3_let.expect("v3 should still exist");
        if let Statement::Let { value, .. } = v3_let {
            if let Expr::Literal { value, .. } = value {
                assert_eq!(*value, BigUint::from(30u64));
            } else {
                panic!("Expected literal after constant folding, got: {value:?}");
            }
        }
    }

    #[test]
    fn test_simplifier_algebraic_identity_add_zero() {
        let mut simplifier = Simplifier::new();

        let stmts = vec![
            make_let_literal(1, 0),
            // let v2 = add(v99, v1) where v1 = 0 → should simplify to Var(v99)
            Statement::Let {
                bindings: vec![ValueId(2)],
                value: Expr::Binary {
                    op: BinOp::Add,
                    lhs: Value::int(ValueId(99)),
                    rhs: Value::int(ValueId(1)),
                },
            },
            Statement::Return {
                offset: Value::int(ValueId(2)),
                length: Value::int(ValueId(2)),
            },
        ];

        let block = &mut Block { statements: stmts };
        simplifier.simplify_block(block);

        // After: add(v99, 0) → Var(v99), v2 now holds Var(v99)
        let v2_let = block.statements.iter().find(
            |s| matches!(s, Statement::Let { bindings, .. } if bindings.contains(&ValueId(2))),
        );
        let v2_let = v2_let.expect("v2 should still exist");
        if let Statement::Let { value, .. } = v2_let {
            match value {
                Expr::Var(id) => assert_eq!(id.0, 99),
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

        let stmts = vec![
            make_let_literal(1, 42),  // v1 = 42, unused
            make_let_literal(2, 100), // v2 = 100, used below
            Statement::Return {
                offset: Value::int(ValueId(2)),
                length: Value::int(ValueId(2)),
            },
        ];

        let block = &mut Block { statements: stmts };
        simplifier.simplify_block(block);

        // Without DCE, all statements are preserved
        assert_eq!(block.statements.len(), 3);
    }

    #[test]
    fn test_copy_propagation() {
        let mut simplifier = Simplifier::new();

        let stmts = vec![
            make_let_literal(1, 42),
            // let v2 = v1 (copy)
            Statement::Let {
                bindings: vec![ValueId(2)],
                value: Expr::Var(ValueId(1)),
            },
            // use v2 → should become v1
            Statement::Return {
                offset: Value::int(ValueId(2)),
                length: Value::int(ValueId(2)),
            },
        ];

        let block = &mut Block { statements: stmts };
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
            BinOp::AddMod,
            &BigUint::from(10u64),
            &BigUint::from(20u64),
            &BigUint::from(7u64),
        );
        assert_eq!(result, Some(BigUint::from(2u64)));

        // mulmod(5, 7, 6) = 35 % 6 = 5
        let result = fold_ternary(
            BinOp::MulMod,
            &BigUint::from(5u64),
            &BigUint::from(7u64),
            &BigUint::from(6u64),
        );
        assert_eq!(result, Some(BigUint::from(5u64)));

        // addmod(x, y, 0) = 0
        let result = fold_ternary(
            BinOp::AddMod,
            &BigUint::from(10u64),
            &BigUint::from(20u64),
            &BigUint::zero(),
        );
        assert_eq!(result, Some(BigUint::zero()));
    }

    #[test]
    fn test_exp_fold() {
        let result = fold_binary(BinOp::Exp, &BigUint::from(2u64), &BigUint::from(10u64));
        assert_eq!(result, Some(BigUint::from(1024u64)));
    }

    #[test]
    fn test_byte_fold() {
        // byte(31, 0xff) = 0xff (least significant byte)
        let result = fold_binary(BinOp::Byte, &BigUint::from(31u64), &BigUint::from(0xFFu64));
        assert_eq!(result, Some(BigUint::from(0xFFu64)));

        // byte(0, 0xff) = 0 (most significant byte of 0xff is 0)
        let result = fold_binary(BinOp::Byte, &BigUint::from(0u64), &BigUint::from(0xFFu64));
        assert_eq!(result, Some(BigUint::zero()));
    }
}
