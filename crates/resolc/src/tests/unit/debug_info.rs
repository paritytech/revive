//! The Solidity compiler tests for debug info.

use revive_llvm_context::OptimizerSettings;

use crate::test_utils::{build_solidity_with_options, sources};

/// Test for a PolkaVM debug line-program with too many instructions.
///
/// At `-O1` through `-O3` and `-Os`, LLVM merges the identical code of many repeated
/// calls. With debug info enabled the merged instruction is attributed to every
/// original call site at once, so the PolkaVM linker reconstructs a very deep
/// inline-frame stack for a single instruction. A debug `resolc` build invoked with `-g`
/// panicked when there were more line-program ops than the runtime parser read per region.
#[test]
fn many_repeated_calls_do_not_overflow_the_line_program() {
    let code = r#"
// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;
interface I {
    function f(uint256) external;
}
contract Test {
    function run(I t, uint256 acc) external {
        t.f(acc); t.f(acc); t.f(acc); t.f(acc); t.f(acc); t.f(acc);
        t.f(acc); t.f(acc); t.f(acc); t.f(acc); t.f(acc); t.f(acc);
        t.f(acc); t.f(acc); t.f(acc); t.f(acc); t.f(acc); t.f(acc);
        t.f(acc); t.f(acc); t.f(acc); t.f(acc); t.f(acc); t.f(acc);
    }
}
"#;

    for opt_level in ['1', '2', '3', 's'] {
        let output = build_solidity_with_options(
            sources(&[("test.sol", code)]),
            Default::default(),
            Default::default(),
            OptimizerSettings::try_from_cli(opt_level).unwrap(),
            true,
            Default::default(),
        )
        .unwrap_or_else(|error| panic!("-O{opt_level} should compile: {error}"));

        assert!(
            !output
                .contracts
                .get("test.sol")
                .expect("`test.sol` should exist")
                .get("Test")
                .expect("contract `Test` should exist")
                .evm
                .as_ref()
                .expect("`evm` field should exist")
                .bytecode
                .as_ref()
                .expect("`bytecode` field should exist")
                .object
                .is_empty(),
            "-O{opt_level} should generate bytecode"
        );
    }
}
