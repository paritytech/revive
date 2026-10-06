// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Heap size: w(p) stores a word at p and returns the 32 bytes at p, so the access [p, p + 32) must
// lie inside the configured heap.  d(n) recurses n deep (an internal function, so on PVM the
// native stack bounds the depth).
contract HS {
    function w(uint256 p) external pure returns (uint256) {
        assembly { mstore(p, 0x1234) return(p, 32) }
    }
    function r(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 0;
        return 1 + r(n - 1);
    }
    function d(uint256 n) external pure returns (uint256) { return r(n); }
}
