//! The Solidity compiler unit tests for runtime code.

use crate::test_utils::{build_solidity, sources};

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

    build_solidity(sources(&[("test.sol", code)]))
        .unwrap()
        .errors
        .iter()
        .find(|error| {
            error
                .to_string()
                .contains("Error: Deploy and runtime code are merged in PVM")
        })
        .unwrap();
}
