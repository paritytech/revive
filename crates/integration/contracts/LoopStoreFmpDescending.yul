/// A descending loop counter reaches the free memory pointer on a later iteration; reverts on a mismatch.
object "LoopStoreFmpDescending" {
  code { datacopy(0, dataoffset("LoopStoreFmpDescending_deployed"), datasize("LoopStoreFmpDescending_deployed")) return(0, datasize("LoopStoreFmpDescending_deployed")) }
  object "LoopStoreFmpDescending_deployed" {
    code {
      mstore(0x40, 0x80)
      let v := calldataload(0)
      for { let p := 0x80 } gt(p, 0x20) { p := sub(p, 0x20) } { mstore(p, v) }
      if iszero(eq(mload(0x40), v)) { revert(0, 0) }
      return(0, 0)
    }
  }
}
