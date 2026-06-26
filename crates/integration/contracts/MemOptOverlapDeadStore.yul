/// Regression fixture for the dead-store-elimination key-mismatch bug in the
/// newyork mem_opt pass. `MemoryOptimizer.pending_stores` is keyed by the exact
/// store offset, but the load handler removed entries by the word-aligned offset.
///
/// For a store at the non-word-aligned offset `0x104`, the exact `mload(0x104)`
/// forwards the stored value but (with the bug) removes the word-aligned key
/// `0x100` instead of `0x104`, so the store is never marked read. The later
/// same-offset `mstore(0x104, v2)` then finds the still-pending entry and
/// dead-eliminates the first store — even though `mload(0x108)` (overlapping,
/// non-forwarded) read its bytes. The eliminated store makes `y` observe fresh
/// memory instead of `v1`, diverging from the EVM reference.
///
/// The `keccak256(0x104, 0x40)` forces the region to big-endian (escaping)
/// memory, so the only behavioural divergence under the bug is the dropped
/// store, not memory byte order. Compiled directly through revive's Yul path so
/// the exact statement shape reaches the newyork mem_opt pass intact.
object "MemOptOverlapDeadStore" {
    code {
        let size := datasize("MemOptOverlapDeadStore_deployed")
        codecopy(0, dataoffset("MemOptOverlapDeadStore_deployed"), size)
        return(0, size)
    }
    object "MemOptOverlapDeadStore_deployed" {
        code {
            let v1 := calldataload(0)
            let v2 := calldataload(32)
            mstore(0x104, v1)
            let x := mload(0x104)
            let y := mload(0x108)
            mstore(0x104, v2)
            let h := keccak256(0x104, 0x40)
            mstore(0, xor(xor(x, y), h))
            return(0, 32)
        }
    }
}
