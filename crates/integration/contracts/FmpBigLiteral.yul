/// Regression (newyork FMP range proof): `is_trusted_fmp_source` trusted a literal
/// free memory pointer of any magnitude, so `mstore(0x40, <literal at or above heap_size>)`
/// left `fmp_could_be_unbounded` false and the surviving `mload(0x40)` was truncated to
/// the `FMP < heap_size` range proof width: `0xdeadbeef` read back as `0x1beef` and
/// `not(0)` as `0x1ffff`. The stores sit in switch cases so `FmpPropagation` cannot
/// forward them to the load. The literal in case 1 stays below the heap size and trusted.
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
