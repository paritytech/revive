// SPDX-License-Identifier: MIT

pragma solidity >=0.8.0;

/// Reproducer from paritytech/security_findings#113.
contract LoopStackTupleWrite {
    uint256 x;
    uint256 y;

    function two(uint256 i, uint256 d) internal pure returns (uint256, uint256) {
        if (d == 0) return (i, i + 1);
        return two(i + 1, d - 1);
    }

    function f(uint256 n) external returns (uint256) {
        for (uint256 i; i < n; ++i) {
            (x, y) = two(i, 1);
        }
        return x;
    }
}
