object "KeccakSingleMsize" {
  code {
    datacopy(0x80, dataoffset("KeccakSingleMsize_deployed"), datasize("KeccakSingleMsize_deployed"))
    return(0x80, datasize("KeccakSingleMsize_deployed"))
  }
  object "KeccakSingleMsize_deployed" {
    code {
      // Eight hashes reach newyork's threshold for the single-word keccak256 helper.
      mstore(0, calldataload(0))
      mstore(0, keccak256(0, 0x20))
      mstore(0, keccak256(0, 0x20))
      mstore(0, keccak256(0, 0x20))
      mstore(0, keccak256(0, 0x20))
      mstore(0, keccak256(0, 0x20))
      mstore(0, keccak256(0, 0x20))
      mstore(0, keccak256(0, 0x20))
      sstore(0, keccak256(0, 0x20))
      mstore(0, msize())
      return(0, 0x20)
    }
  }
}
