// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

/// Soundness PoC: newyork's `narrow_function_returns` only inspects the
/// function's fall-through `return_values`, ignoring values returned by EARLY
/// `leave` (return) statements. When the fall-through returns a small constant
/// (here `return 0`, min_width I1) but earlier branches return full-width
/// values, the function's return type is narrowed to i32 and EVERY early-leave
/// result is truncated to 32 bits. `op(a,b)` for a = 2^256-1 returns
/// 0xffffffff instead of 2^256-1.
contract MultiLeaveReturnNarrow {
    function run(uint256 op, uint256 a, uint256 b) external pure returns (uint256) {
        if (op == 0) return a + b;
        if (op == 1) return a - b;
        if (op == 2) return a * b;
        if (op == 3) return a / b;
        if (op == 4) return a % b;
        return 0;            // small-constant fall-through
    }
}
