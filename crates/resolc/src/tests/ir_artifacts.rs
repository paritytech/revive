//! The Solidity compiler unit tests for IR artifacts.
//! The tests check if the IR artifacts are kept in the final output.

#[test]
fn yul() {
    let code = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function main() public view returns (uint) {
        return 42;
    }
}
    "#;

    let build = super::build_solidity(
        super::sources(&[("test.sol", code)]),
        Default::default(),
        Default::default(),
        revive_llvm_context::OptimizerSettings::cycles(),
    )
    .expect("Test failure");

    assert!(
        !build
            .contracts
            .get("test.sol")
            .expect("Always exists")
            .get("Test")
            .expect("Always exists")
            .ir_optimized
            .is_empty(),
        "Yul IR is missing"
    );
}
