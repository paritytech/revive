object "MsizeCopy" {
  code { let s := datasize("MsizeCopy_deployed") codecopy(0, dataoffset("MsizeCopy_deployed"), s) return(0, s) }
  object "MsizeCopy_deployed" {
    code {
      let op := calldataload(0)
      let r := 0
      switch op
      case 0 { calldatacopy(0x81, 0, 5) r := msize() }   // dest 0x81 len 5 -> end 0x86 -> EVM 0xA0
      case 1 { mstore(0x80, 1) mcopy(0x141, 0x80, 7) r := msize() }  // dest 0x141 len 7 -> 0x148 -> 0x160
      case 2 { calldatacopy(0x100, 0, 33) r := msize() }  // 0x100+33=0x121 -> 0x140
      default { r := 0 }
      mstore(0x400, r)
      return(0x400, 32)
    }
  }
}
