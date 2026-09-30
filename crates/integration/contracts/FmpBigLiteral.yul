/// Literal free memory pointers at or above the heap size must not be range proved.
object "FmpBigLiteral" {
  code { datacopy(0, dataoffset("FmpBigLiteral_deployed"), datasize("FmpBigLiteral_deployed")) return(0, datasize("FmpBigLiteral_deployed")) }
  object "FmpBigLiteral_deployed" {
    code {
      switch calldataload(0)
      case 1 { mstore(0x40, 0x80) }
      case 2 { mstore(0x40, 0xdeadbeef) }
      case 3 { mstore(0x40, not(0)) }
      mstore(0, mload(0x40))
      return(0, 32)
    }
  }
}
