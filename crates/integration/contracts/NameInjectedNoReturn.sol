// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Reproducer from paritytech/security_findings#111.
contract NameInjectedNoReturn {
    function g$llvm_NoReturn_llvm$x(uint256 n) internal pure returns (uint256) {
        return n == 0 ? 1 : g$llvm_NoReturn_llvm$x(n - 1) + 1;
    }

    function f(uint256 n) external pure returns (uint256 r) {
        r = g$llvm_NoReturn_llvm$x(n) * 2;
        require(r == 2 * (n + 1));
    }
}
