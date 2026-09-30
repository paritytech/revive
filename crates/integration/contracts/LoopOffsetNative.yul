/// A loop writes words `0x80` and `0xa0`; a literal load of `0xa0` must not be native.
object "LoopOffsetNative" {
  code { datacopy(0, dataoffset("LoopOffsetNative_deployed"), datasize("LoopOffsetNative_deployed")) return(0, datasize("LoopOffsetNative_deployed")) }
  object "LoopOffsetNative_deployed" {
    code {
      let value := calldataload(0)
      for { let i := 0x80 } lt(i, 0xc0) { i := add(i, 0x20) } {
        mstore(i, value)
      }
      mstore(0, mload(0xa0))
      return(0, 32)
    }
  }
}
