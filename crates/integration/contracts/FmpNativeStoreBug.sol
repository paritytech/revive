// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract FmpNativeStoreBug {
    // Recursive — solc can't inline.
    function loop(uint depth) internal pure returns (uint) {
        if (depth == 0) return 0;
        return loop(depth - 1) + depth;
    }

    fallback() external {
        uint256 v;
        uint256 depth;
        assembly {
            v := calldataload(0)
            depth := calldataload(0x20)
            mstore(0x40, v)
        }
        // Recursive call: mem_opt clears memory_state, no escape so
        // fmp_word_escapes stays false, fmp_native_safe stays true.
        uint256 unused = loop(depth);
        uint256 r;
        assembly {
            r := mload(0x40)
            // use unused so solc doesn't DCE the call
            r := add(r, mul(unused, 0))
            mstore(0x100, r)
            return(0x100, 32)
        }
    }
}
