/// An `mcopy` from `0` with a dynamic length overwrites the free memory pointer; reverts on a mismatch.
object "CopyFmpDynamicLength" {
  code { datacopy(0, dataoffset("CopyFmpDynamicLength_deployed"), datasize("CopyFmpDynamicLength_deployed")) return(0, datasize("CopyFmpDynamicLength_deployed")) }
  object "CopyFmpDynamicLength_deployed" {
    code {
      let v := calldataload(0)
      mstore(0xc0, v)
      mcopy(0, 0x80, calldatasize())
      if iszero(eq(mload(0x40), v)) { revert(0, 0) }
      return(0, 0)
    }
  }
}
