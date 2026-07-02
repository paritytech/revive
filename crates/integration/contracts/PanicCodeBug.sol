// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Soundness PoC: the panic-pattern outliner in
/// `simplify.rs::find_panic_pattern_backwards` extracts the panic code
/// from `mstore(4, code)` via `code_val.to_u64_digits().first()` and a
/// `<= 0xFF` check. That returns the LEAST-significant u64 digit, so any
/// 256-bit `code` value whose low byte is in `[0, 0xFF]` is mis-classified
/// as a small canonical Solidity panic code regardless of its higher bits.
///
/// Codegen for `Statement::PanicRevert { code }` emits the canonical
/// 36-byte Solidity panic format (selector + zero-padded `code`), so the
/// high bits of the original mstored value are silently dropped from the
/// revert data observed by the caller. EVM emits the original bytes.
contract PanicCodeBug {
    fallback() external {
        assembly {
            // canonical panic selector at offset 0
            mstore(0, 0x4e487b7100000000000000000000000000000000000000000000000000000000)
            // "code" value 0x10_00000000_00000000_00000000_00000042 — its low
            // byte (0x42) passes the simplifier's `<= 0xFF` check, but byte
            // position 20 of the revert encoding holds the 0x10 nibble.
            mstore(4, 0x10000000000000000000000000000042)
            revert(0, 0x24)
        }
    }
}
