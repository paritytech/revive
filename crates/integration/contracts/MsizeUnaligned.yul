object "MsizeUnaligned" {
  code { let s := datasize("MsizeUnaligned_deployed") codecopy(0, dataoffset("MsizeUnaligned_deployed"), s) return(0, s) }
  object "MsizeUnaligned_deployed" {
    code {
      let op := calldataload(0)
      let x := calldataload(32)
      let r := 0
      switch op
      case 0 { mstore(0x81, x) r := msize() }    // EVM: ceil(0xA1)->0xC0 (192)
      case 1 { mstore8(0x85, x) r := msize() }   // EVM: ceil(0x86)->0xA0 (160)
      case 2 { mstore(0x80, x) r := msize() }    // aligned: 0xA0 (160)
      case 3 { mstore(0xc1, x) r := msize() }    // EVM: ceil(0xE1)->0x100 (256)
      default { r := 0 }
      mstore(0x400, r)
      return(0x400, 32)
    }
  }
}
