// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

library L { function f(uint256 x) external pure returns (uint256) { return x * 3; } }
contract A { function g(uint256 x) external pure returns (uint256) { return L.f(x); } }
contract B { function g(uint256 x) external pure returns (uint256) { return L.f(x) + 1; } }
