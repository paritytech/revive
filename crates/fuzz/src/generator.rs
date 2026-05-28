//! `Arbitrary`-driven Solidity generator. Every [`SolidityCase`] is
//! one of N templates ([`crate::templates`]) sharing the wire shape
//! `constructor(uint256 seed)` + `fn_0(uint256 arg)`. Multiple
//! templates × per-template op menus give libFuzzer enough surface
//! to drive into different resolc lowering paths.

use arbitrary::{Arbitrary, Unstructured};

use crate::templates::{self, TemplateKind};

/// Number of actions issued against each deployed contract.
const ACTION_COUNT_RANGE: std::ops::RangeInclusive<u8> = 2..=4;

/// 1-in-5 chance to pull a sentinel instead of a uniform random
/// 32-byte word. Surfaces a given corner pair (e.g. `INT_MIN % -1`)
/// in ~1 minute at 8 threads × ~30 cases/sec; uniform-random
/// would essentially never hit it.
const INTERESTING_RATIO_NUM: u8 = 1;
const INTERESTING_RATIO_DEN: u8 = 5;

/// 256-bit big-endian `int256` sentinels: zero, ±1, ±2, INT_MIN,
/// INT_MAX, powers of two at word-half boundaries, alternating bits.
fn interesting_value(index: u8) -> [u8; 32] {
    let mut v = [0u8; 32];
    match index {
        0 => {}
        1 => v[31] = 1,
        2 => v[31] = 2,
        3 => v.fill(0xff), // -1
        4 => {
            v.fill(0xff);
            v[31] = 0xfe;
        } // -2
        5 => v[0] = 0x80,  // INT_MIN
        6 => {
            v[0] = 0x80;
            v[31] = 1;
        } // INT_MIN + 1
        7 => {
            v.fill(0xff);
            v[0] = 0x7f;
        } // INT_MAX
        8 => {
            v.fill(0xff);
            v[0] = 0x7f;
            v[31] = 0xfe;
        } // INT_MAX - 1
        9 => v[15] = 0x01, // 2^128
        10 => v[16..].fill(0xff), // 2^128 - 1
        11 => v[23] = 0x01, // 2^64
        12 => v[24..].fill(0xff), // 2^64 - 1
        13 => v.fill(0x55),
        14 => v.fill(0xaa),
        _ => unreachable!("interesting_value index out of range"),
    }
    v
}

const N_INTERESTING: u8 = 15;

/// Sentinel with `INTERESTING_RATIO_NUM/INTERESTING_RATIO_DEN`
/// probability, otherwise uniform random.
fn pick_operand(u: &mut Unstructured<'_>) -> arbitrary::Result<[u8; 32]> {
    if u.ratio(INTERESTING_RATIO_NUM, INTERESTING_RATIO_DEN)? {
        let idx = u.int_in_range(0..=(N_INTERESTING - 1))?;
        Ok(interesting_value(idx))
    } else {
        <[u8; 32]>::arbitrary(u)
    }
}

#[derive(Debug, Clone)]
pub struct SolidityCase {
    pub contract_name: String,
    pub source: String,
    pub constructor_args: Vec<[u8; 32]>,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub argument: [u8; 32],
}

impl<'a> Arbitrary<'a> for SolidityCase {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let template = TemplateKind::arbitrary(u)?;
        let rendered = templates::render(template, u)?;
        let constructor_seed = pick_operand(u)?;
        let action_count = u.int_in_range(ACTION_COUNT_RANGE)? as usize;
        let mut actions = Vec::with_capacity(action_count);
        for _ in 0..action_count {
            actions.push(Action {
                argument: pick_operand(u)?,
            });
        }
        Ok(Self {
            contract_name: rendered.name,
            source: rendered.source,
            constructor_args: vec![constructor_seed],
            actions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_smoke() {
        let mut seed = [0u8; 4096];
        for byte in seed.iter_mut().enumerate() {
            *byte.1 = (byte.0 * 17 + 31) as u8;
        }
        let mut u = Unstructured::new(&seed);
        let case = SolidityCase::arbitrary(&mut u).expect("arbitrary should succeed");
        // Lock the wire shape — observer assumes both.
        assert!(case.source.contains("contract "));
        assert!(case.source.contains("fn_0("));
        assert!(case.source.contains("constructor(uint256 seed)"));
        assert!(!case.actions.is_empty());
        assert_eq!(case.constructor_args.len(), 1);
    }

    /// Spot-check the sentinel encodings — easy to typo a byte index.
    #[test]
    fn interesting_pool_shape() {
        let zero = interesting_value(0);
        assert!(zero.iter().all(|&b| b == 0));

        let neg_one = interesting_value(3);
        assert!(neg_one.iter().all(|&b| b == 0xff));

        let int_min = interesting_value(5);
        assert_eq!(int_min[0], 0x80);
        assert!(int_min[1..].iter().all(|&b| b == 0));

        let int_max = interesting_value(7);
        assert_eq!(int_max[0], 0x7f);
        assert!(int_max[1..].iter().all(|&b| b == 0xff));
    }
}
