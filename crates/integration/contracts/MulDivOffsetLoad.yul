/// `mul(a, a)` with the forwarded `a` 2^128 wraps to 0, so the store goes to `div(0, 0)`, which is 0, and the load from 1 must read memory.
object "MulDivOffsetLoad" {
  code { datacopy(0, dataoffset("MulDivOffsetLoad_deployed"), datasize("MulDivOffsetLoad_deployed")) return(0, datasize("MulDivOffsetLoad_deployed")) }
  object "MulDivOffsetLoad_deployed" {
    code {
      mstore(0, 0x100000000000000000000000000000000)
      let a := mload(0)
      let p := mul(a, a)
      mstore(div(p, p), calldataload(0))
      mstore(0x80, mload(1))
      return(0x80, 32)
    }
  }
}
