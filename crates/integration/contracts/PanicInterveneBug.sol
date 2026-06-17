// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Soundness PoC: `simplify.rs::find_panic_pattern_backwards` walks
/// backward from `revert(0, 0x24)` skipping every mstore whose offset
/// is not exactly 0 or 4, and the final `only_safe` gate only checks
/// that intermediate statements are `Let`/`MStore`/`Expression`. An
/// `mstore(p, …)` with `p ∈ (0, 4) ∪ (4, 0x24)` partially overwrites
/// the panic-encoded revert data EVM emits, but the simplifier still
/// matches the canonical panic shape and replaces the whole sequence
/// with `Statement::PanicRevert { code }` — dropping the corrupting
/// mstore from the revert data observed by the caller.
contract PanicInterveneBug {
    fallback() external {
        assembly {
            // Canonical panic selector + code prelude.
            mstore(0, 0x4e487b7100000000000000000000000000000000000000000000000000000000)
            mstore(4, 0x01)
            // Unaligned mstore overwriting bytes 7..38 — bytes 7..35 of
            // this end up in the revert(0, 0x24) data on EVM. PVM emits
            // canonical PanicRevert(0x01) instead, dropping these bytes.
            mstore(7, 0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef)
            revert(0, 0x24)
        }
    }
}
