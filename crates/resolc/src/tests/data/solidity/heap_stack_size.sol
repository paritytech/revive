// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract HeapStackSize {
    function storeAndReturnWord(uint256 offset) external pure returns (uint256) {
        assembly {
            mstore(offset, 0x1234)
            return(offset, 32)
        }
    }

    function recurse(uint256 depth) internal pure returns (uint256) {
        if (depth == 0) return 0;
        return 1 + recurse(depth - 1);
    }

    function recursionDepth(uint256 depth) external pure returns (uint256) {
        return recurse(depth);
    }
}
