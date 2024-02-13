//!
//! The Solidity compiler unit tests for the optimizer.
//!

#![cfg(test)]

use std::collections::BTreeMap;

use crate::solc::pipeline::Pipeline as SolcPipeline;

pub const SOURCE_CODE: &str = r#"
// SPDX-License-Identifier: MIT

pragma solidity >=0.5.0;

contract Test {
    uint8 constant ARRAY_SIZE = 40;
    uint128 constant P = 257;
    uint128 constant MODULO = 1000000007;

    function complex() public pure returns(uint64) {
        uint8[ARRAY_SIZE] memory array;
        // generate array where first half equals second
        for(uint8 i = 0; i < ARRAY_SIZE; i++) {
            array[i] = (i % (ARRAY_SIZE / 2)) * (255 / (ARRAY_SIZE / 2 - 1));
        }

        bool result = true;
        for(uint8 i = 0; i < ARRAY_SIZE/2; i++) {
            result = result && hash(array, 0, i + 1) == hash(array, ARRAY_SIZE/2, ARRAY_SIZE/2 + i + 1)
            &&  hash(array, i, ARRAY_SIZE/2) == hash(array, i + ARRAY_SIZE/2, ARRAY_SIZE);
        }
        if (result) {
            return 1;
        } else {
            return 0;
        }
    }

    function hash(uint8[ARRAY_SIZE] memory array, uint8 begin, uint8 end) private pure returns(uint128) {
        uint128 h = 0;
        for(uint8 i = begin; i < end; i++) {
            h = (h * P + array[i]) % MODULO;
        }
        return h;
    }
}
"#;

#[test]
fn optimizer() {
    let mut sources = BTreeMap::new();
    sources.insert("test.sol".to_owned(), SOURCE_CODE.to_owned());

    let build_unoptimized = super::build_solidity(
        sources.clone(),
        BTreeMap::new(),
        None,
        SolcPipeline::Yul,
        era_compiler_llvm_context::OptimizerSettings::none(),
    )
    .expect("Build failure");
    let build_optimized_for_cycles = super::build_solidity(
        sources.clone(),
        BTreeMap::new(),
        None,
        SolcPipeline::Yul,
        era_compiler_llvm_context::OptimizerSettings::cycles(),
    )
    .expect("Build failure");
    let build_optimized_for_size = super::build_solidity(
        sources,
        BTreeMap::new(),
        None,
        SolcPipeline::Yul,
        era_compiler_llvm_context::OptimizerSettings::size(),
    )
    .expect("Build failure");

    let size_when_unoptimized = build_unoptimized
        .contracts
        .as_ref()
        .expect("Missing field `contracts`")
        .get("test.sol")
        .expect("Missing file `test.sol`")
        .get("Test")
        .expect("Missing contract `test.sol:Test`")
        .evm
        .as_ref()
        .expect("Missing EVM data")
        .bytecode
        .as_ref()
        .expect("Missing bytecode")
        .object
        .len();
    let size_when_optimized_for_cycles = build_optimized_for_cycles
        .contracts
        .as_ref()
        .expect("Missing field `contracts`")
        .get("test.sol")
        .expect("Missing file `test.sol`")
        .get("Test")
        .expect("Missing contract `test.sol:Test`")
        .evm
        .as_ref()
        .expect("Missing EVM data")
        .bytecode
        .as_ref()
        .expect("Missing bytecode")
        .object
        .len();
    let size_when_optimized_for_size = build_optimized_for_size
        .contracts
        .as_ref()
        .expect("Missing field `contracts`")
        .get("test.sol")
        .expect("Missing file `test.sol`")
        .get("Test")
        .expect("Missing contract `test.sol:Test`")
        .evm
        .as_ref()
        .expect("Missing EVM data")
        .bytecode
        .as_ref()
        .expect("Missing bytecode")
        .object
        .len();

    assert!(
        size_when_optimized_for_cycles < size_when_unoptimized,
        "Expected the cycles-optimized bytecode to be smaller than the unoptimized. Optimized: {}B, Unoptimized: {}B", size_when_optimized_for_cycles, size_when_unoptimized,
    );
    assert!(
        size_when_optimized_for_size < size_when_unoptimized,
        "Expected the size-optimized bytecode to be smaller than the unoptimized. Optimized: {}B, Unoptimized: {}B", size_when_optimized_for_size, size_when_unoptimized,
    );
}
