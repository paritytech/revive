// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Soundness PoC: `mem_opt::optimize_statements` MStore8 handler only
/// invalidates `memory_state[word_offset(offset)]`. An unaligned
/// `mstore(0x70, AA…)` leaves a tracked entry at word 0x60 with
/// `tracked.offset = 0x70`, whose 32-byte write range covers byte
/// 0x80. A later `mstore8(0x80, 0xCC)` overwrites that byte but only
/// removes `memory_state[0x80]`, not the overlapping entry at
/// `memory_state[0x60]`. A subsequent `mload(0x70)` matches the
/// stale tracked entry by exact-offset comparison and is forwarded
/// to the pre-overwrite value, dropping the single-byte overwrite.
/// EVM observes byte 0x80 = `0xCC` in the actual memory read.
contract UnalignedMStore8Bug {
    function bug() external pure returns (bytes32 r) {
        assembly {
            mstore(0x70, 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa)
            mstore8(0x80, 0xcc)
            r := mload(0x70)
        }
    }
}
