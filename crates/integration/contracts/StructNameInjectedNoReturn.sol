// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// A struct name containing an LLVM attribute marker.
contract StructNameInjectedNoReturn {
    struct S$llvm_NoReturn_llvm$x {
        uint256 a;
        uint256 b;
    }

    function f(uint256 n) external pure returns (S$llvm_NoReturn_llvm$x memory s) {
        s = S$llvm_NoReturn_llvm$x(n, n + 1);
    }
}
