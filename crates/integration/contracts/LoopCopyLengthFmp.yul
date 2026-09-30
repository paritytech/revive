/// A copy whose length is a loop counter reaches the free memory pointer; reverts on a mismatch.
object "LoopCopyLengthFmp" {
  code { datacopy(0, dataoffset("LoopCopyLengthFmp_deployed"), datasize("LoopCopyLengthFmp_deployed")) return(0, datasize("LoopCopyLengthFmp_deployed")) }
  object "LoopCopyLengthFmp_deployed" {
    code {
      mstore(0x40, 0x80)
      for { let n := 0x40 } lt(n, 0x42) { n := add(n, 1) } { calldatacopy(0, 0, n) }
      if iszero(eq(mload(0x40), or(shl(248, shr(248, calldataload(0x40))), 0x80))) { revert(0, 0) }
      return(0, 0)
    }
  }
}
