/// A loop counter starting at `0x40` overwrites the free memory pointer. The pointer is not set first, so no stale value can be forwarded and only the heap analysis keeps the read from being truncated; reverts on a mismatch.
object "LoopStoreFmpWord" {
  code { datacopy(0, dataoffset("LoopStoreFmpWord_deployed"), datasize("LoopStoreFmpWord_deployed")) return(0, datasize("LoopStoreFmpWord_deployed")) }
  object "LoopStoreFmpWord_deployed" {
    code {
      let v := calldataload(0)
      for { let p := 0x40 } lt(p, 0x41) { p := add(p, 1) } { mstore(p, v) }
      if iszero(eq(mload(0x40), v)) { revert(0, 0) }
      return(0, 0)
    }
  }
}
