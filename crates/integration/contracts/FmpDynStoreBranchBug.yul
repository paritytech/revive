/// Regression (newyork FMP constant forwarding): a full-word store whose offset the
/// optimizer cannot resolve may land on the free-memory-pointer word `[0x40, 0x60)`.
/// The straight-line propagation invalidated its tracked pointer on such a store, but
/// the region predicate consulted for `if`, `switch`, `for` and block bodies did not,
/// so the stale constant `0x80` was forwarded to the `mload(0x40)` after the branch.
/// The first call takes the benign path and reads `0x80`; the second stores `0xa0`
/// through a calldata-supplied offset of `0x40` and must read `0xa0` back.
object "FmpDynStoreBranchBug" {
  code { datacopy(0, dataoffset("FmpDynStoreBranchBug_deployed"), datasize("FmpDynStoreBranchBug_deployed")) return(0, datasize("FmpDynStoreBranchBug_deployed")) }
  object "FmpDynStoreBranchBug_deployed" {
    code {
      mstore(0x40, 0x80)
      if calldataload(0) { mstore(calldataload(32), 0xa0) }
      mstore(0, mload(0x40))
      return(0, 32)
    }
  }
}
