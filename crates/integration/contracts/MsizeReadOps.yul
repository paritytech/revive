object "MsizeReadOps" {
  code { let s := datasize("MsizeReadOps_deployed") codecopy(0, dataoffset("MsizeReadOps_deployed"), s) return(0, s) }
  object "MsizeReadOps_deployed" {
    code {
      let op := calldataload(0)
      let r := 0
      switch op
      case 0 { mstore(0,0) let h := keccak256(0x81, 5) r := msize() }   // reads 0x81..0x86 -> EVM 0xA0
      case 1 { log0(0x81, 7) r := msize() }                             // 0x81..0x88 -> EVM 0xA0
      case 2 { let h := keccak256(0xc1, 33) r := msize() }              // 0xc1..0xe2 -> EVM 0x100
      default { r := 0 }
      mstore(0x400, r)
      return(0x400, 32)
    }
  }
}
