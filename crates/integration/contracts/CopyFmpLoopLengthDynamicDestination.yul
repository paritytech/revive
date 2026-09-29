/// A `calldatacopy` with a loop counter length from `0` to a calldata destination.
object "CopyFmpLoopLengthDynamicDestination" {
  code { datacopy(0, dataoffset("CopyFmpLoopLengthDynamicDestination_deployed"), datasize("CopyFmpLoopLengthDynamicDestination_deployed")) return(0, datasize("CopyFmpLoopLengthDynamicDestination_deployed")) }
  object "CopyFmpLoopLengthDynamicDestination_deployed" {
    code {
      for { let n := 0 } lt(n, 0x40) { n := add(n, 0x20) } {
          calldatacopy(calldataload(32), 0, n)
      }
      mstore(0, mload(0x40))
      return(0, 32)
    }
  }
}
