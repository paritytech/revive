// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract FmpRangeProofBug {
    fallback() external {
        assembly {
            let v := calldataload(0)
            mstore(0x40, v)
            // Force fmp_word_escapes via a staticcall whose args cover
            // memory[0..0x80] (so 0x40 is in the escape range). That
            // disables FMP-native-mode storage and forces ByteSwap mode
            // where the range-proof bug lives.
            let _success := staticcall(gas(), origin(), 0, 0x80, 0, 0)
            let r := mload(0x40)
            mstore(0x100, r)
            return(0x100, 32)
        }
    }
}
