//! Yul case generator driven by [`arbitrary::Arbitrary`].
//!
//! Each [`YulCase`] is a self-contained Yul object with a runtime
//! body that loads 3 calldata words + 2 storage slots, runs a
//! random sequence of pure builtins, persists the last two values
//! back to storage, and returns the final word.
//!
//! Restricted to pure arithmetic / bitwise / comparison builtins —
//! no environment reads (`address`, `gas`, …), no side effects
//! (`log*`, `*call`, `create*`, …), no scratch memory writes. Keeps
//! every case deterministic across both backends.
//!
//! Storage seeds are baked into the source as Yul literals (not
//! constructor calldata) because EVM init code reads ctor args via
//! `codecopy` from appended bytes while pallet-revive delivers them
//! as calldata — same bytes, different access path.

use std::fmt::Write;

use arbitrary::{Arbitrary, Unstructured};

/// Number of actions (calls) issued per case.
const ACTION_COUNT_RANGE: std::ops::RangeInclusive<u8> = 2..=4;

/// Number of randomly-generated operations in the runtime body.
const OP_COUNT_RANGE: std::ops::RangeInclusive<u8> = 4..=12;

/// Bytes of calldata per call: three 32-byte operands.
pub const ACTION_CALLDATA_LEN: usize = 96;

/// Probability of selecting a 256-bit operand from the curated
/// interesting-value pool when generating a literal source operand.
const INTERESTING_RATIO_NUM: u8 = 1;
const INTERESTING_RATIO_DEN: u8 = 5;

/// Probability of generating a literal source operand at all (vs.
/// picking a previously-declared variable).
const LITERAL_RATIO_NUM: u8 = 1;
const LITERAL_RATIO_DEN: u8 = 4;

/// 256-bit big-endian sentinels — same pool as the Solidity-side
/// `generator::interesting_value`, kept in sync by construction.
fn interesting_value(index: u8) -> [u8; 32] {
    let mut v = [0u8; 32];
    match index {
        0 => {}
        1 => v[31] = 1,
        2 => v[31] = 2,
        3 => v.fill(0xff),                                // -1 / u256::MAX
        4 => { v.fill(0xff); v[31] = 0xfe; }              // -2
        5 => v[0] = 0x80,                                 // INT_MIN
        6 => { v[0] = 0x80; v[31] = 1; }                  // INT_MIN + 1
        7 => { v.fill(0xff); v[0] = 0x7f; }               // INT_MAX
        8 => { v.fill(0xff); v[0] = 0x7f; v[31] = 0xfe; } // INT_MAX - 1
        9 => v[15] = 0x01,                                // 2^128
        10 => v[16..].fill(0xff),                         // 2^128 - 1
        11 => v[23] = 0x01,                               // 2^64
        12 => v[24..].fill(0xff),                         // 2^64 - 1
        13 => v.fill(0x55),
        14 => v.fill(0xaa),
        _ => unreachable!("interesting_value index out of range"),
    }
    v
}

const N_INTERESTING: u8 = 15;

fn pick_word(u: &mut Unstructured<'_>) -> arbitrary::Result<[u8; 32]> {
    if u.ratio(INTERESTING_RATIO_NUM, INTERESTING_RATIO_DEN)? {
        let index = u.int_in_range(0..=(N_INTERESTING - 1))?;
        Ok(interesting_value(index))
    } else {
        <[u8; 32]>::arbitrary(u)
    }
}

/// Render a 256-bit value as a Yul hex literal (`0x…`).
fn hex_literal(buf: &mut String, value: &[u8; 32]) {
    let _ = write!(buf, "0x");
    let mut leading = true;
    for &byte in value {
        if leading && byte == 0 {
            continue;
        }
        if leading {
            let _ = write!(buf, "{byte:x}");
            leading = false;
        } else {
            let _ = write!(buf, "{byte:02x}");
        }
    }
    if leading {
        buf.push('0');
    }
}

/// Pure scalar Yul builtins the generator emits. Arity in `arity()`.
#[derive(Debug, Clone, Copy)]
enum Op {
    IsZero,
    Not,
    Add,
    Sub,
    Mul,
    Div,
    Sdiv,
    Mod,
    Smod,
    Lt,
    Gt,
    Slt,
    Sgt,
    Eq,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Sar,
    SignExtend,
    Byte,
    AddMod,
    MulMod,
}

impl Op {
    const ALL: &'static [Op] = &[
        Op::IsZero, Op::Not,
        Op::Add, Op::Sub, Op::Mul, Op::Div, Op::Sdiv, Op::Mod, Op::Smod,
        Op::Lt, Op::Gt, Op::Slt, Op::Sgt, Op::Eq,
        Op::And, Op::Or, Op::Xor, Op::Shl, Op::Shr, Op::Sar,
        Op::SignExtend, Op::Byte,
        Op::AddMod, Op::MulMod,
    ];

    fn name(self) -> &'static str {
        match self {
            Op::IsZero => "iszero",
            Op::Not => "not",
            Op::Add => "add",
            Op::Sub => "sub",
            Op::Mul => "mul",
            Op::Div => "div",
            Op::Sdiv => "sdiv",
            Op::Mod => "mod",
            Op::Smod => "smod",
            Op::Lt => "lt",
            Op::Gt => "gt",
            Op::Slt => "slt",
            Op::Sgt => "sgt",
            Op::Eq => "eq",
            Op::And => "and",
            Op::Or => "or",
            Op::Xor => "xor",
            Op::Shl => "shl",
            Op::Shr => "shr",
            Op::Sar => "sar",
            Op::SignExtend => "signextend",
            Op::Byte => "byte",
            Op::AddMod => "addmod",
            Op::MulMod => "mulmod",
        }
    }

    fn arity(self) -> usize {
        match self {
            Op::IsZero | Op::Not => 1,
            Op::AddMod | Op::MulMod => 3,
            _ => 2,
        }
    }
}

#[derive(Debug, Clone)]
enum Operand {
    Var(String),
    Literal([u8; 32]),
}

impl Operand {
    fn render(&self, buf: &mut String) {
        match self {
            Operand::Var(name) => buf.push_str(name),
            Operand::Literal(bytes) => hex_literal(buf, bytes),
        }
    }
}

/// `let vN := <op>(<args...>)`.
#[derive(Debug, Clone)]
struct Step {
    result: String,
    op: Op,
    args: Vec<Operand>,
}

#[derive(Debug, Clone)]
pub struct YulCase {
    pub contract_name: String,
    pub source: String,
    /// One [`ACTION_CALLDATA_LEN`]-byte calldata blob per call.
    pub actions: Vec<Vec<u8>>,
}

impl<'a> Arbitrary<'a> for YulCase {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let seed0 = pick_word(u)?;
        let seed1 = pick_word(u)?;

        // Initial variable pool: `a`/`b`/`c` are the calldata words,
        // `s0`/`s1` are the storage slots. Op steps append `vN`.
        let mut variables: Vec<String> =
            vec!["a".into(), "b".into(), "c".into(), "s0".into(), "s1".into()];

        let op_count = u.int_in_range(OP_COUNT_RANGE)? as usize;
        let mut steps = Vec::with_capacity(op_count);
        for index in 0..op_count {
            let op = *u.choose(Op::ALL)?;
            let mut args = Vec::with_capacity(op.arity());
            for _ in 0..op.arity() {
                args.push(pick_operand(u, &variables)?);
            }
            let name = format!("v{index}");
            steps.push(Step { result: name.clone(), op, args });
            variables.push(name);
        }

        let action_count = u.int_in_range(ACTION_COUNT_RANGE)? as usize;
        let mut actions = Vec::with_capacity(action_count);
        for _ in 0..action_count {
            let mut calldata = Vec::with_capacity(ACTION_CALLDATA_LEN);
            for _ in 0..3 {
                calldata.extend_from_slice(&pick_word(u)?);
            }
            actions.push(calldata);
        }

        let contract_name = format!("FuzzYul_{:08x}", u.arbitrary::<u32>()?);
        let source = render_object(&contract_name, &seed0, &seed1, &steps);
        Ok(Self { contract_name, source, actions })
    }
}

fn pick_operand(
    u: &mut Unstructured<'_>,
    variables: &[String],
) -> arbitrary::Result<Operand> {
    if u.ratio(LITERAL_RATIO_NUM, LITERAL_RATIO_DEN)? {
        Ok(Operand::Literal(pick_word(u)?))
    } else {
        let name = u.choose(variables)?.clone();
        Ok(Operand::Var(name))
    }
}

fn render_object(name: &str, seed0: &[u8; 32], seed1: &[u8; 32], steps: &[Step]) -> String {
    let mut s = String::with_capacity(1024);
    let _ = writeln!(s, "object \"{name}\" {{");
    // Deploy: seed storage, copy the runtime body out, return it.
    let _ = writeln!(s, "    code {{");
    s.push_str("        sstore(0, ");
    hex_literal(&mut s, seed0);
    s.push_str(")\n");
    s.push_str("        sstore(1, ");
    hex_literal(&mut s, seed1);
    s.push_str(")\n");
    let _ = writeln!(s, "        let _size := datasize(\"{name}_deployed\")");
    let _ = writeln!(s, "        let _off := dataoffset(\"{name}_deployed\")");
    let _ = writeln!(s, "        datacopy(0, _off, _size)");
    let _ = writeln!(s, "        return(0, _size)");
    let _ = writeln!(s, "    }}");

    // Runtime: load inputs → run ops → persist last two → return.
    let _ = writeln!(s, "    object \"{name}_deployed\" {{");
    let _ = writeln!(s, "        code {{");
    let _ = writeln!(s, "            let a := calldataload(0)");
    let _ = writeln!(s, "            let b := calldataload(32)");
    let _ = writeln!(s, "            let c := calldataload(64)");
    let _ = writeln!(s, "            let s0 := sload(0)");
    let _ = writeln!(s, "            let s1 := sload(1)");
    for step in steps {
        s.push_str("            let ");
        s.push_str(&step.result);
        s.push_str(" := ");
        s.push_str(step.op.name());
        s.push('(');
        for (i, arg) in step.args.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            arg.render(&mut s);
        }
        s.push_str(")\n");
    }
    // Fall back to the calldata words when too few ops to source
    // both slots — keeps the storage update input-dependent.
    let (persist0, persist1, ret) = match steps.len() {
        0 => ("a".to_string(), "b".to_string(), "a".to_string()),
        1 => (steps[0].result.clone(), "a".to_string(), steps[0].result.clone()),
        n => (
            steps[n - 1].result.clone(),
            steps[n - 2].result.clone(),
            steps[n - 1].result.clone(),
        ),
    };
    let _ = writeln!(s, "            sstore(0, {persist0})");
    let _ = writeln!(s, "            sstore(1, {persist1})");
    let _ = writeln!(s, "            mstore(0, {ret})");
    let _ = writeln!(s, "            return(0, 32)");
    let _ = writeln!(s, "        }}");
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "}}");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_smoke() {
        let mut seed = [0u8; 4096];
        for (i, byte) in seed.iter_mut().enumerate() {
            *byte = (i * 31 + 7) as u8;
        }
        let mut u = Unstructured::new(&seed);
        let case = YulCase::arbitrary(&mut u).expect("arbitrary should succeed");
        assert!(case.source.contains("object \""));
        assert!(case.source.contains("_deployed"));
        assert!(case.source.contains("calldataload(0)"));
        assert!(case.source.contains("return(0, 32)"));
        for action in &case.actions {
            assert_eq!(action.len(), ACTION_CALLDATA_LEN);
        }
    }

    #[test]
    fn hex_literal_zero() {
        let mut buf = String::new();
        hex_literal(&mut buf, &[0u8; 32]);
        assert_eq!(buf, "0x0");
    }

    #[test]
    fn hex_literal_one() {
        let mut buf = String::new();
        let mut v = [0u8; 32];
        v[31] = 1;
        hex_literal(&mut buf, &v);
        assert_eq!(buf, "0x1");
    }

    #[test]
    fn hex_literal_all_ff() {
        let mut buf = String::new();
        hex_literal(&mut buf, &[0xff; 32]);
        assert_eq!(buf, format!("0x{}", "ff".repeat(32)));
    }
}
