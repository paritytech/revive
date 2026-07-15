//! Source-text differential cases.
//!
//! The `arbitrary`-driven [`crate::SolidityCase`] treats fuzzer bytes as a
//! decision tape that selects one of the built-in templates. This module
//! provides the complementary path: the bytes *are* the Solidity source (e.g.
//! LLM-generated contracts seeded into the corpus), run against the same
//! differential oracle.
//!
//! Every source-text contract must follow the wire shape the observers assume:
//! a `constructor(uint256 seed)` and a `function fn_0(uint256 arg)` entrypoint.
//! The constructor seed and action arguments are a fixed set of `int256` corner
//! values rather than fuzzed bytes, so a corpus entry is exactly one `.sol`
//! file and byte-level mutation edits Solidity text directly.

use crate::generator::{interesting_value, Action};
use crate::SolidityCase;

/// Constructor seed passed to `constructor(uint256 seed)`: value `1` — nonzero
/// so storage-initializing contracts take a non-default path.
const CONSTRUCTOR_SEED_INDEX: u8 = 1;

/// `int256` corner values fed to `fn_0` on successive actions: `0, 1, -1,
/// INT_MIN, INT_MAX` (indices into [`interesting_value`]). These sentinels are
/// the ones most likely to expose signed-arithmetic lowering divergences.
const ACTION_ARG_INDICES: [u8; 5] = [0, 1, 3, 5, 7];

/// Marker every entrypoint contract must expose. Used to locate the entrypoint
/// contract name and as a cheap structural check.
const ENTRYPOINT_MARKER: &str = "fn_0";

/// Contract-declaration keyword, with a trailing space so it does not match
/// identifiers such as `contractName`.
const CONTRACT_KEYWORD: &str = "contract ";

impl SolidityCase {
    /// Build a differential case from raw Solidity source.
    ///
    /// Returns `None` when no entrypoint contract (one defining
    /// [`ENTRYPOINT_MARKER`]) can be located — e.g. after a mutation destroyed
    /// it — so the harness can cheaply skip the input.
    pub fn from_source(source: &str) -> Option<Self> {
        let contract_name = extract_entrypoint_name(source)?;
        let actions = ACTION_ARG_INDICES
            .iter()
            .map(|&index| Action {
                argument: interesting_value(index),
            })
            .collect();
        Some(Self {
            contract_name,
            source: source.to_owned(),
            constructor_args: vec![interesting_value(CONSTRUCTOR_SEED_INDEX)],
            actions,
        })
    }
}

/// Name of the contract that declares the [`ENTRYPOINT_MARKER`] function.
///
/// Heuristic: find the marker, then the nearest preceding `contract <Name>`.
/// A wrong guess is harmless — the compile step keys its output by contract
/// name and reports a missing name as a skipped (compile-failed) case.
pub fn extract_entrypoint_name(source: &str) -> Option<String> {
    let marker = source.find(ENTRYPOINT_MARKER)?;
    let prefix = &source[..marker];
    let keyword = prefix.rfind(CONTRACT_KEYWORD)?;
    let after = &prefix[keyword + CONTRACT_KEYWORD.len()..];
    let name: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTRACT: &str = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8;
contract Fuzz {
    uint256 public slot;
    constructor(uint256 seed) { slot = seed; }
    function fn_0(uint256 arg) external returns (uint256) {
        unchecked { slot = slot + arg; }
        return slot;
    }
}
"#;

    #[test]
    fn extracts_entrypoint_contract_name() {
        assert_eq!(extract_entrypoint_name(CONTRACT).as_deref(), Some("Fuzz"));
    }

    #[test]
    fn picks_contract_defining_the_marker() {
        let source = "contract Helper { } contract Entry { function fn_0(uint256 a) external {} }";
        assert_eq!(extract_entrypoint_name(source).as_deref(), Some("Entry"));
    }

    #[test]
    fn no_entrypoint_returns_none() {
        assert!(extract_entrypoint_name("contract X { function g() external {} }").is_none());
        assert!(SolidityCase::from_source("not solidity at all").is_none());
    }

    #[test]
    fn from_source_uses_fixed_wire_shape() {
        let case = SolidityCase::from_source(CONTRACT).expect("has entrypoint");
        assert_eq!(case.contract_name, "Fuzz");
        assert_eq!(case.constructor_args.len(), 1);
        assert_eq!(case.actions.len(), ACTION_ARG_INDICES.len());
    }
}
