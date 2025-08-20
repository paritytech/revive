//! The Solidity compiler unit tests for IR artifacts.
//! The tests check if the IR artifacts are kept in the final output.

#![cfg(test)]

use std::collections::BTreeMap;

#[test]
fn yul() {
    let source_code = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function main() public view returns (uint) {
        return 42;
    }
}
    "#;

    let mut sources = BTreeMap::new();
    sources.insert("test.sol".to_owned(), source_code.to_owned());

    let build = super::build_solidity(
        sources,
        Default::default(),
        None,
        revive_llvm_context::OptimizerSettings::cycles(),
    )
    .expect("Test failure");

    assert!(
        build
            .contracts
            .as_ref()
            .expect("Always exists")
            .get("test.sol")
            .expect("Always exists")
            .get("Test")
            .expect("Always exists")
            .ir_optimized
            .is_some(),
        "Yul IR is missing"
    );
}
