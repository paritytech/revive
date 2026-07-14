// SPDX-License-Identifier: MIT

pragma solidity ^0.8.28;

/// Soundness PoC (newyork heap optimization): a full-word `mload` at a
/// non-word-aligned offset that overlaps a word the heap analysis classified
/// as native (little-endian, byte-swap elided) reads that word's bytes in the
/// wrong order.
///
/// `mstore(0x20, PAT)` is word-aligned and does not escape, so newyork keeps
/// it little-endian (native mode, no byte-swap). `mload(0x08)` reads
/// `[0x08, 0x28)`, overlapping the native word at `0x20`, but is itself
/// unaligned so it lowers to a big-endian (byte-swapped) read. The two
/// accesses disagree on byte order for the shared bytes, so the load returns
/// byte-swapped garbage. The fix taints every word an unaligned full-word
/// access covers, forcing those words big-endian for all accesses.
contract UnalignedMload {
    function f() external pure returns (bytes32 r) {
        assembly {
            mstore(0x20, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)
            r := mload(0x08)
        }
    }
}
