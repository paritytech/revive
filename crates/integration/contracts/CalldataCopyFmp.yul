/// A copy with a STATIC destination in [0x40, 0x60) but a DYNAMIC length
/// clobbers the free-memory-pointer slot with arbitrary bytes. A subsequent
/// mload(0x40) must observe the full 256-bit word (EVM), but newyork applies
/// its FMP range proof and truncates it because `fmp_could_be_unbounded` is
/// not set for the dynamic-length copy case (only the constant-length case).
object "CalldataCopyFmp" {
  code { datacopy(0, dataoffset("CalldataCopyFmp_deployed"), datasize("CalldataCopyFmp_deployed")) return(0, datasize("CalldataCopyFmp_deployed")) }
  object "CalldataCopyFmp_deployed" {
    code {
      let len := calldataload(0)
      calldatacopy(0x40, 32, len)
      let r := mload(0x40)
      mstore(0, r)
      return(0, 32)
    }
  }
}
