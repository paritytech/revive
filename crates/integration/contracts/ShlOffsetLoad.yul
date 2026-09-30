/// `shl(shift, 1)` with the forwarded shift 5 stores at 0x20, so the load from 10 must read memory.
object "ShlOffsetLoad" {
  code { datacopy(0, dataoffset("ShlOffsetLoad_deployed"), datasize("ShlOffsetLoad_deployed")) return(0, datasize("ShlOffsetLoad_deployed")) }
  object "ShlOffsetLoad_deployed" {
    code {
      mstore(0, 5)
      let shift := mload(0)
      mstore(shl(shift, 1), calldataload(0))
      mstore(0x80, mload(10))
      return(0x80, 32)
    }
  }
}
