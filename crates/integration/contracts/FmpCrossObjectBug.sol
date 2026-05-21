// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract FmpCrossObjectBug {
    // Recursive so solc can't inline; the function survives to
    // newyork IR as `fun_rec` with parameter ValueIds starting at 0,
    // colliding with the deploy (parent) object's `v0 := 0x80`.
    function rec(uint256 value, uint256 depth) internal view returns (uint256 r) {
        if (depth == 0) {
            assembly {
                mstore(0x40, value)
                // Force the same byte-swap-mode path as FmpRangeProofBug
                // so the range proof is the only optimization controlling
                // the loaded value.
                let _success := staticcall(gas(), origin(), 0, 0x80, 0, 0)
                r := mload(0x40)
            }
            return r;
        }
        return rec(value, depth - 1);
    }

    fallback() external {
        uint256 value;
        uint256 depth;
        assembly {
            value := calldataload(0)
            depth := calldataload(0x20)
        }
        uint256 result = rec(value, depth);
        assembly {
            mstore(0x100, result)
            return(0x100, 32)
        }
    }
}
