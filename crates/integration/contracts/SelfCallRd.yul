object "SelfCallRd" {
  code { datacopy(0, dataoffset("SelfCallRd_deployed"), datasize("SelfCallRd_deployed")) return(0, datasize("SelfCallRd_deployed")) }
  object "SelfCallRd_deployed" {
    code {
      let flag := calldataload(0)
      switch flag
      // leaf: return a chunk of size = calldataload(32) (bounded), filled with calldataload(64)
      case 0 {
        let sz := and(calldataload(32), 0xFF)
        mstore(0, calldataload(64))
        mstore(32, not(calldataload(64)))
        mstore(64, calldataload(64))
        return(0, sz)
      }
      // caller: self-call leaf, then returndatacopy(dest, off, len) with bounded params
      default {
        mstore(0x80, 0)
        mstore(0xA0, calldataload(32))
        mstore(0xC0, calldataload(64))
        let ok := call(gas(), address(), 0, 0x80, 96, 0x100, 0)
        let rds := returndatasize()
        let off := and(calldataload(96), 0xFF)
        let len := and(calldataload(128), 0xFF)
        returndatacopy(0x200, off, len)
        let r := xor(mload(0x200), add(rds, ok))
        mstore(0, r)
        return(0, 32)
      }
    }
  }
}
