/// A store at `0x30` bound outside the branch overwrites part of the free memory pointer; reverts on a mismatch.
object "ScratchStoreBranchFmp" {
  code { datacopy(0, dataoffset("ScratchStoreBranchFmp_deployed"), datasize("ScratchStoreBranchFmp_deployed")) return(0, datasize("ScratchStoreBranchFmp_deployed")) }
  object "ScratchStoreBranchFmp_deployed" {
    code {
      mstore(0x40, 0x80)
      let offset := 0x30
      if calldataload(64) {
        let v := calldataload(0)
        mstore(offset, v)
        if iszero(eq(mload(0x40), or(shl(128, v), 0x80))) { revert(0, 0) }
      }
      return(0, 0)
    }
  }
}
