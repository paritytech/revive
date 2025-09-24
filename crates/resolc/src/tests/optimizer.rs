//! The Solidity compiler unit tests for the optimizer.

#[test]
fn optimizer() {
    let sources = &[(
        "test.sol",
        r#"
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
}"#,
    )];

    let build_unoptimized = super::build_solidity_with_options(
        super::sources(sources),
        Default::default(),
        Default::default(),
        revive_llvm_context::OptimizerSettings::none(),
        true,
    )
    .expect("Build failure");
    let build_optimized_for_cycles =
        super::build_solidity(super::sources(sources)).expect("Build failure");
    let build_optimized_for_size = super::build_solidity_with_options(
        super::sources(sources),
        Default::default(),
        Default::default(),
        revive_llvm_context::OptimizerSettings::size(),
        true,
    )
    .expect("Build failure");

    let size_when_unoptimized = build_unoptimized
        .contracts
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
        "Expected the cycles-optimized bytecode to be smaller than the unoptimized. Optimized: {size_when_optimized_for_cycles}B, Unoptimized: {size_when_unoptimized}B",
    );
    assert!(
        size_when_optimized_for_size < size_when_unoptimized,
        "Expected the size-optimized bytecode to be smaller than the unoptimized. Optimized: {size_when_optimized_for_size}B, Unoptimized: {size_when_unoptimized}B",
    );
}
