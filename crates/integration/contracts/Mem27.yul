object "Mem27" {
  code { datacopy(0, dataoffset("Mem27_deployed"), datasize("Mem27_deployed")) return(0, datasize("Mem27_deployed")) }
  object "Mem27_deployed" {
    code {
      let r := 0
      r := xor(r, mload(16))
      r := xor(r, keccak256(16, 29))
      r := xor(r, mload(16))
      r := add(r, sload(13))
      r := xor(r, keccak256(0, 15))
      mstore(0, r)
      return(0, 32)
    }
  }
}
