object "Store8Fmp" {
  code { datacopy(0, dataoffset("Store8Fmp_deployed"), datasize("Store8Fmp_deployed")) return(0, datasize("Store8Fmp_deployed")) }
  object "Store8Fmp_deployed" {
    code {
      mstore8(0x40, 0xAB)
      let r := mload(0x40)
      mstore(0, r)
      return(0, 32)
    }
  }
}
