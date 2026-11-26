object "KeccakPairMsize" {
  code {
    datacopy(0x80, dataoffset("KeccakPairMsize_deployed"), datasize("KeccakPairMsize_deployed"))
    return(0x80, datasize("KeccakPairMsize_deployed"))
  }
  object "KeccakPairMsize_deployed" {
    code {
      mstore(0, calldataload(0))
      mstore(0x20, 1)
      sstore(0, keccak256(0, 0x40))
      let s := msize()
      mstore(0, s)
      return(0, 0x20)
    }
  }
}
