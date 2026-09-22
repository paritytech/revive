/// Regression (newyork heap analysis): loop-carried offsets were seeded with the
/// initializer's static value, so every iteration of `mstore(i, value)` was analyzed
/// as a store to `0x80`. Word `0xa0` stayed a native little-endian candidate because
/// only the literal `mload(0xa0)` was seen to touch it, while the loop wrote it
/// byte-swapped through the dynamic store path, so the load returned the value with
/// its bytes reversed.
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
