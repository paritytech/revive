/// A calldata supplied store offset of `0x40` inside a branch must reach the pointer read.
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
