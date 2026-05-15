// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

// Differential guard against a latent soundness footgun in the codegen for
// __revive_caller: before the fix the helper wrote its result through the
// shared @__address_spill_buffer LLVM global while declaring
// memory(inaccessiblemem: read). The same global is also written by the
// inlined tx.origin and address(this) patterns, so the function attribute
// was a contract violation from LLVM's point of view (globals are Other
// memory, not inaccessiblemem). Any optimizer pass that exploited the wrong
// attribute to CSE / hoist a load of the spill buffer across a
// __revive_caller() call would have miscompiled the surrounding tx.origin /
// address(this) read.
//
// The fix moves __revive_caller's output to a function-local alloca so the
// attribute matches the body. This contract exercises every shape that
// previously shared the spill buffer.
contract CallerOriginAliasing {
    function caller_then_origin() external view returns (address, address) {
        return (msg.sender, tx.origin);
    }

    function origin_then_caller() external view returns (address, address) {
        return (tx.origin, msg.sender);
    }

    function caller_address_origin() external view returns (address, address, address) {
        return (msg.sender, address(this), tx.origin);
    }

    function repeated_caller() external view returns (address, address, address) {
        return (msg.sender, msg.sender, msg.sender);
    }
}
