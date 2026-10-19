// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Reproducer from paritytech/security_findings#111.
contract NameOverlappingLlvmMarkers {
    function f(uint256 a) external pure returns (uint256 r) {
        assembly {
            function $llvm_llvm$(x) -> y {
                y := 1
                if x { y := add($llvm_llvm$(sub(x, 1)), 2) }
            }
            r := $llvm_llvm$(a)
        }
    }
}
