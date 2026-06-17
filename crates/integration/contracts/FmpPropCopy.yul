/// Track an FMP value, then overwrite the FMP slot via a calldatacopy whose
/// source is beyond calldatasize (zero-fill). A stale FMP-propagation would
/// forward the tracked value; EVM reads the (now zeroed) slot.
object "FmpPropCopy" {
  code { datacopy(0, dataoffset("FmpPropCopy_deployed"), datasize("FmpPropCopy_deployed")) return(0, datasize("FmpPropCopy_deployed")) }
  object "FmpPropCopy_deployed" {
    code {
      mstore(0x40, 0x1234)
      calldatacopy(0x40, 1000, 32)   // src 1000 >= calldatasize -> zero-fills heap[0x40..0x60]
      let p := mload(0x40)            // EVM: 0; stale FmpProp would give 0x1234
      mstore(0x80, p)
      return(0x80, 32)
    }
  }
}
