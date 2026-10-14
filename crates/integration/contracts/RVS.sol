// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract RVS {
    struct S { uint256 a; uint256 b; }
    mapping(uint256 => S) m;
    function g(uint256 k, uint256 n) internal returns (S storage s) {
        if (n > 0) g(k, n - 1);
        s = m[k];
        s.a = 7;
    }
    function run(uint256 k, uint256 n) external returns (uint256) {
        S storage s = g(k, n);
        s.b = 9;
        return m[k].b;
    }
}
