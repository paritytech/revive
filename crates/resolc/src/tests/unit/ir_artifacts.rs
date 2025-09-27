//! The Solidity compiler unit tests for IR artifacts.
//! The tests check if the IR artifacts are kept in the final output.

use crate::test_utils::{build_solidity, sources};

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

    let build = build_solidity(sources(&[("test.sol", code)])).expect("Test failure");

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
