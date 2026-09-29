/// An `mcopy` to a parameter that one call site sets to `0x40` overwrites the free memory pointer; reverts on a mismatch.
object "CopyFmpUntrustedParameter" {
  code { datacopy(0, dataoffset("CopyFmpUntrustedParameter_deployed"), datasize("CopyFmpUntrustedParameter_deployed")) return(0, datasize("CopyFmpUntrustedParameter_deployed")) }
  object "CopyFmpUntrustedParameter_deployed" {
    code {
      function copy_word(destination) { mcopy(destination, 0xa0, 0x20) }
      mstore(0x40, 0xa0)
      let v := calldataload(0)
      mstore(0xa0, v)
      copy_word(mload(0x40))
      copy_word(and(calldataload(64), 0x40))
      if iszero(eq(mload(0x40), v)) { revert(0, 0) }
      return(0, 0)
    }
  }
}
