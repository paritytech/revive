//! The Solidity compiler unit tests for runtime code.

#![cfg(test)]

use std::collections::BTreeMap;

#[test]
#[should_panic(expected = "runtimeCode is not supported")]
fn default() {
    let source_code = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract A {}

contract Test {
    function main() public pure returns(bytes memory) {
        return type(A).runtimeCode;
    }
}
    "#;

    let mut sources = BTreeMap::new();
    sources.insert("test.sol".to_owned(), source_code.to_owned());

    super::build_solidity(
        sources,
        BTreeMap::new(),
        None,
        revive_llvm_context::OptimizerSettings::cycles(),
    )
    .expect("Test failure");
}
