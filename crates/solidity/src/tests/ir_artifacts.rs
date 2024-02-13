//!
//! The Solidity compiler unit tests for IR artifacts.
//!
//! The tests check if the IR artifacts are kept in the final output.
//!

#![cfg(test)]

use std::collections::BTreeMap;

use crate::solc::pipeline::Pipeline as SolcPipeline;

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
        BTreeMap::new(),
        None,
        SolcPipeline::Yul,
        era_compiler_llvm_context::OptimizerSettings::cycles(),
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
    assert!(
        build
            .contracts
            .as_ref()
            .expect("Always exists")
            .get("test.sol")
            .expect("Always exists")
            .get("Test")
            .expect("Always exists")
            .evm
            .as_ref()
            .expect("EVM object is missing")
            .assembly
            .is_none(),
        "EVMLA IR is present although not requested"
    );
}

#[test]
fn evmla() {
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
        BTreeMap::new(),
        None,
        SolcPipeline::EVMLA,
        era_compiler_llvm_context::OptimizerSettings::cycles(),
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
            .evm
            .as_ref()
            .expect("EVM object is missing")
            .assembly
            .is_some(),
        "EVMLA IR is missing",
    );
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
            .is_none(),
        "Yul IR is present although not requested",
    );
}
