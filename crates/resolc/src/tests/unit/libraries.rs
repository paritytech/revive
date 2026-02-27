//! The Solidity compiler unit tests for libraries.

use revive_solc_json_interface::SolcStandardJsonInputSettingsLibraries;

use crate::test_utils::build_solidity_and_detect_missing_libraries;

pub const CODE: &str = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// A simple library with at least one external method
library SimpleLibrary {
    function add(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }
}

// A contract calling that library
contract SimpleContract {
    using SimpleLibrary for uint256;

    function performAlgorithm(uint256 a, uint256 b) public pure returns (uint256) {
        uint sum = 0;
        if (a > b) {
            while (true) {
                sum += a.add(b);
            }
        }
        return sum;
    }
}"#;

#[test]
fn not_specified() {
    let output =
        build_solidity_and_detect_missing_libraries(&[("test.sol", CODE)], Default::default())
            .unwrap();
    assert!(
        output
            .contracts
            .get("test.sol")
            .expect("Always exists")
            .get("SimpleContract")
            .expect("Always exists")
            .missing_libraries
            .contains("test.sol:SimpleLibrary"),
        "Missing library not detected"
    );
}

#[test]
fn specified() {
    let mut libraries = SolcStandardJsonInputSettingsLibraries::default();
    libraries
        .as_inner_mut()
        .entry("test.sol".to_string())
        .or_default()
        .entry("SimpleLibrary".to_string())
        .or_insert("0x00000000000000000000000000000000DEADBEEF".to_string());

    let output =
        build_solidity_and_detect_missing_libraries(&[("test.sol", CODE)], libraries.clone())
            .unwrap();
    assert!(
        output
            .contracts
            .get("test.sol")
            .expect("Always exists")
            .get("SimpleContract")
            .expect("Always exists")
            .missing_libraries
            .is_empty(),
        "The list of missing libraries must be empty"
    );
}
