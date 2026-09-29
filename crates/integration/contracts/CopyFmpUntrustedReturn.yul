/// An `mcopy` to a call result that is `0x40` overwrites the free memory pointer; reverts on a mismatch.
object "CopyFmpUntrustedReturn" {
  code { datacopy(0, dataoffset("CopyFmpUntrustedReturn_deployed"), datasize("CopyFmpUntrustedReturn_deployed")) return(0, datasize("CopyFmpUntrustedReturn_deployed")) }
  object "CopyFmpUntrustedReturn_deployed" {
    code {
      function pick(p) -> r { r := and(p, 0x40) }
      function copy_word(destination) { mcopy(destination, 0xa0, 0x20) }
      mstore(0x40, 0xa0)
      let v := calldataload(0)
      mstore(0xa0, v)
      copy_word(add(mload(0x40), 0))
      copy_word(pick(calldataload(64)))
      if iszero(eq(mload(0x40), v)) { revert(0, 0) }
      return(0, 0)
    }
  }
}
