/// Soundness PoC (newyork FMP constant forwarding): an unaligned `mstore` that
/// overlaps the free-memory-pointer word `[0x40, 0x60)` inside a *conditional*
/// branch corrupts the pointer on the taken path, yet `FmpPropagation`'s
/// region-level invalidation (`region_modifies_fmp`) recognized only stores
/// tagged `FreePointerSlot` — the overlap store at `0x38` is tagged `Scratch` —
/// so the stale constant `0x80` was forwarded to the `mload(0x40)` after the
/// branch and the store through it silently succeeded where EVM runs out of
/// gas on the unbounded memory expansion.
///
/// With calldata word 0 zero the branch is skipped and both stacks return 32
/// zero bytes; with it non-zero `mstore(56, not(0))` overwrites the FMP word
/// and `mstore(mload(0x40), 0x42)` must run out of gas on both stacks.
object "FmpBranchOverlapOogBug" {
  code { datacopy(0, dataoffset("FmpBranchOverlapOogBug_deployed"), datasize("FmpBranchOverlapOogBug_deployed")) return(0, datasize("FmpBranchOverlapOogBug_deployed")) }
  object "FmpBranchOverlapOogBug_deployed" {
    code {
      mstore(0x40, 0x80)
      if calldataload(0) { mstore(56, not(0)) }
      mstore(mload(0x40), 0x42)
      mstore(0, 0)
      return(0, 32)
    }
  }
}
