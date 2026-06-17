// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Soundness PoC: `heap_opt.rs::fmp_native_safe()` does NOT check
/// `tainted_regions` or `escaping_regions`. A `revert(0, 96)` that
/// covers the FMP slot (0x40) only calls `mark_escaping_range`
/// (line 241-243), which adds the FMP word to `escaping_regions` /
/// `tainted_regions` — but `fmp_native_safe()` only looks at four flags
/// (variable_accessed_offsets, has_dynamic_accesses,
/// has_return_covering_fmp, min_dynamic_escape_start) and returns
/// true when the static `revert(0, 96)` set none of them.
///
/// `Statement::Return` correctly sets `has_return_covering_fmp` in this
/// case (line 252-254). `Statement::Revert` does not, so a contract
/// that reverts with data covering the FMP gets the FMP slot stored
/// as native i32 little-endian (4 bytes at heap[0x40..0x44]) instead
/// of the big-endian 32-byte word EVM produces.
///
/// EVM revert data [0x40..0x60) = 32-byte BE encoding of FMP value.
/// PVM revert data [0x40..0x60) = 4 bytes LE i32, then 28 bytes
/// of uninitialized/zero heap memory.
contract FmpRevertBug {
    fallback() external {
        assembly {
            // Set FMP to 0xdeadbeef. With newyork's InlineNative mode,
            // this becomes a 4-byte i32 LE store at heap[0x40..0x44].
            mstore(0x40, 0xdeadbeef)
            // Revert with 96 bytes covering [0x00..0x60). EVM caller
            // sees 32-byte BE encoding of FMP at bytes [0x40..0x60).
            // PVM caller sees the 4-byte LE encoding at [0x40..0x44)
            // followed by 28 uninitialized bytes — different bytes.
            revert(0, 96)
        }
    }
}
