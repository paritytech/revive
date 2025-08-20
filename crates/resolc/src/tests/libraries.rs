//! The Solidity compiler unit tests for libraries.

#![cfg(test)]

use std::collections::BTreeMap;

use revive_solc_json_interface::SolcStandardJsonInputSettingsLibraries;

pub const LIBRARY_TEST_SOURCE: &str = r#"
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
}
    "#;

#[test]
fn not_specified() {
    let mut sources = BTreeMap::new();
    sources.insert("test.sol".to_owned(), LIBRARY_TEST_SOURCE.to_owned());

    let output =
        super::build_solidity_and_detect_missing_libraries(sources.clone(), Default::default())
            .expect("Test failure");
    assert!(
        output
            .contracts
            .as_ref()
            .expect("Always exists")
            .get("test.sol")
            .expect("Always exists")
            .get("SimpleContract")
            .expect("Always exists")
            .missing_libraries
            .as_ref()
            .expect("Always exists")
            .contains("test.sol:SimpleLibrary"),
        "Missing library not detected"
    );
}

#[test]
fn specified() {
    let mut sources = BTreeMap::new();
    sources.insert("test.sol".to_owned(), LIBRARY_TEST_SOURCE.to_owned());

    let mut libraries = SolcStandardJsonInputSettingsLibraries::default();
    libraries
        .as_inner_mut()
        .entry("test.sol".to_string())
        .or_default()
        .entry("SimpleLibrary".to_string())
        .or_insert("0x00000000000000000000000000000000DEADBEEF".to_string());

    let output =
        super::build_solidity_and_detect_missing_libraries(sources.clone(), libraries.clone())
            .expect("Test failure");
    assert!(
        output
            .contracts
            .as_ref()
            .expect("Always exists")
            .get("test.sol")
            .expect("Always exists")
            .get("SimpleContract")
            .expect("Always exists")
            .missing_libraries
            .as_ref()
            .cloned()
            .unwrap_or_default()
            .is_empty(),
        "The list of missing libraries must be empty"
    );
}
