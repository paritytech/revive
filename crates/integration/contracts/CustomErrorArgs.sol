// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

/// Probe: custom error revert data must match EVM (selector + ABI-encoded args)
/// for edge-case argument values. Targets newyork's custom-error outlining.
contract CustomErrorArgs {
    error E1(uint256 a);
    error E2(uint256 a, uint256 b, address c);
    error E3(uint8 x, bytes32 y);
    function f(uint256 sel, uint256 a, uint256 b) external pure {
        if (sel == 0) revert E1(a);
        if (sel == 1) revert E2(a, b, address(uint160(a)));
        if (sel == 2) revert E3(uint8(a), bytes32(b));
        revert E1(~a);
    }
}
