/// A call returning into `0` with a dynamic length overwrites the free memory pointer; reverts on a mismatch.
object "CallReturnFmpDynamicLength" {
  code { datacopy(0, dataoffset("CallReturnFmpDynamicLength_deployed"), datasize("CallReturnFmpDynamicLength_deployed")) return(0, datasize("CallReturnFmpDynamicLength_deployed")) }
  object "CallReturnFmpDynamicLength_deployed" {
    code {
      let v := calldataload(0)
      mstore(0xc0, v)
      if iszero(staticcall(gas(), 4, 0x80, calldatasize(), 0, calldatasize())) { revert(0, 0) }
      if iszero(eq(mload(0x40), v)) { revert(0, 0) }
      return(0, 0)
    }
  }
}
