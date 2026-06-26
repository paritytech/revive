// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Soundness PoC: `mem_opt.rs` only updates `memory_state[word_offset]` when
/// recording an mstore, where `word_offset = static_offset / 32 * 32`. An
/// *unaligned* `mstore(0x70, v)` writes 32 bytes from byte 0x70 to 0x90, so
/// it overwrites the lower half of the word starting at 0x80. mem_opt only
/// touches `memory_state[0x60]` and leaves `memory_state[0x80]` intact.
///
/// A subsequent `mload(0x80)` finds the still-cached tracked entry at 0x80
/// (offset == 0x80, no aliasing check across overlap with 0x70) and is
/// forwarded to the *pre-overwrite* stored value. EVM reads the actual
/// memory which contains the lower half of the unaligned store plus the
/// untouched upper half of the original.
contract UnalignedMStoreBug {
    function bug() external pure returns (bytes32 r) {
        assembly {
            // 32 byte sentinel at the aligned word 0x80.
            mstore(0x80, 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa)
            // Unaligned write: 16 bytes of 0xbb at 0x70..0x80 and 16 bytes of
            // 0xcc at 0x80..0x90 (overwriting the lower half of the sentinel).
            mstore(0x70, 0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbcccccccccccccccccccccccccccccccc)
            // mem_opt forwards the stale sentinel, missing the partial overwrite.
            r := mload(0x80)
        }
    }
}
