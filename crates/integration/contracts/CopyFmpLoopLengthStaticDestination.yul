/// An `mcopy` onto `0x40` with a loop counter length from `0`.
object "CopyFmpLoopLengthStaticDestination" {
  code { datacopy(0, dataoffset("CopyFmpLoopLengthStaticDestination_deployed"), datasize("CopyFmpLoopLengthStaticDestination_deployed")) return(0, datasize("CopyFmpLoopLengthStaticDestination_deployed")) }
  object "CopyFmpLoopLengthStaticDestination_deployed" {
    code {
      mstore(0x80, calldataload(0))
      for { let n := 0 } lt(n, 0x40) { n := add(n, 0x20) } {
          mcopy(0x40, 0x80, n)
      }
      mstore(0, mload(0x40))
      return(0, 32)
    }
  }
}
