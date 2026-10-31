// SPDX-License-Identifier: MIT

pragma solidity >=0.8.0;

/// Reproducer from paritytech/security_findings#113.
contract LoopStackStorageWrite {
    uint256 x;

    function f(uint256 n) external returns (uint256) {
        for (uint256 i; i < n; ++i) {
            x = i + 1;
        }
        return x;
    }
}
