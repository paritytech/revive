/// Store a value at 0x80, then overwrite [0x80,0xA0) with calldata via calldatacopy.
/// A stale load-forward would return the original stored value.
object "LoadFwdCopy" {
  code { datacopy(0, dataoffset("LoadFwdCopy_deployed"), datasize("LoadFwdCopy_deployed")) return(0, datasize("LoadFwdCopy_deployed")) }
  object "LoadFwdCopy_deployed" {
    code {
      mstore(0x80, 0x1234)
      calldatacopy(0x80, 0, 32)
      let p := mload(0x80)        // EVM: calldata[0..32]; stale fwd: 0x1234
      mstore(0x100, p)
      return(0x100, 32)
    }
  }
}
