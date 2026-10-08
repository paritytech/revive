// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Reproducer from paritytech/security_findings#111.
contract NameInjectedUnknownAttribute {
    function h$llvm_Bogus_llvm$x(uint256 n) internal pure returns (uint256) { return n == 0 ? 7 : h$llvm_Bogus_llvm$x(n - 1); }
    function f(uint256 n) external pure returns (uint256) { return h$llvm_Bogus_llvm$x(n); }
}
