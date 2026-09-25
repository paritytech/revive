/// A loop counter starting at `0x30` partially overwrites the free memory pointer; reverts on a mismatch.
object "LoopStoreFmpOverlap" {
  code { datacopy(0, dataoffset("LoopStoreFmpOverlap_deployed"), datasize("LoopStoreFmpOverlap_deployed")) return(0, datasize("LoopStoreFmpOverlap_deployed")) }
  object "LoopStoreFmpOverlap_deployed" {
    code {
      mstore(0x40, 0x80)
      let v := calldataload(0)
      for { let p := 0x30 } lt(p, 0x31) { p := add(p, 1) } { mstore(p, v) }
      if iszero(eq(mload(0x40), or(shl(128, v), 0x80))) { revert(0, 0) }
      return(0, 0)
    }
  }
}
