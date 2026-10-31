// SPDX-License-Identifier: MIT

pragma solidity >=0.8.0;

/// Reproducer from paritytech/security_findings#113.
contract LoopStackPure {
    function q(uint256 n, uint256 s) external pure returns (uint256) {
        for (uint256 i; i < n; ++i) {
            uint256 x = i * 3;
            uint256 y = x ^ s;
            s = y + 1;
        }
        return s;
    }
}
