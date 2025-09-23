//! The Solidity compiler unit tests for runtime code.

#[test]
fn default() {
    let code = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract A {}

contract Test {
    function main() public pure returns(bytes memory) {
        return type(A).runtimeCode;
    }
}
    "#;

    super::build_solidity(
        super::sources(&[("test.sol", code)]),
        Default::default(),
        Default::default(),
        revive_llvm_context::OptimizerSettings::cycles(),
    )
    .expect("Test failure")
    .errors
    .iter()
    .find(|error| {
        error
            .to_string()
            .contains("Error: Deploy and runtime code are merged in PVM")
    })
    .unwrap();
}
