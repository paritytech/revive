// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Soundness PoC: function parameter used only as `mload` offset gets
/// silently narrowed from i256 to i64 by `narrow_function_params`. The
/// call-site bare truncate drops the upper 192 bits BEFORE the use-site
/// `safe_truncate_int_to_xlen` can observe them, so `mload(2^64)` aliases
/// to `mload(0)` and returns the zero-initialised scratch slot instead of
/// OOGing on memory expansion the way EVM does.
///
/// `fetch` is intentionally recursive so the early newyork inliner refuses
/// to inline it, preserving the narrowable `mload(x)`-only parameter shape.
contract ParamMload {
    function fetch(uint depth, uint x) internal pure returns (uint r) {
        if (depth == 0) {
            assembly { r := mload(x) }
            return r;
        }
        return fetch(depth - 1, x);
    }

    function tryFetch(uint x) external pure returns (uint) {
        return fetch(1, x);
    }
}
