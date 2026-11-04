// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract RecursiveCallResult {
    uint256 public calls;
    function walk(uint256 d, uint256 a) internal returns (uint256 r) {
        if (d == 0) return a;
        r = walk(d - 1, a + 1);
        calls += 1;
    }
    function f(uint256 d, uint256 a) external returns (uint256) {
        return walk(d, a);
    }
}
