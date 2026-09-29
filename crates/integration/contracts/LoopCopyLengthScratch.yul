/// A copy whose length is a loop counter from `0x40` also writes word `0x20`, so the literal store and load of `0x20` must not use native byte order; reverts on a mismatch.
object "LoopCopyLengthScratch" {
  code { datacopy(0, dataoffset("LoopCopyLengthScratch_deployed"), datasize("LoopCopyLengthScratch_deployed")) return(0, datasize("LoopCopyLengthScratch_deployed")) }
  object "LoopCopyLengthScratch_deployed" {
    code {
      mstore(0x20, 1)
      for { let length := 0x40 } lt(length, 0x41) { length := add(length, 1) } {
        calldatacopy(0, 0, length)
      }
      if iszero(eq(mload(0x20), calldataload(0x20))) { revert(0, 0) }
      return(0, 0)
    }
  }
}
