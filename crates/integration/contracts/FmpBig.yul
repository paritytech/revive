object "FmpBig" {
  code { datacopy(0, dataoffset("FmpBig_deployed"), datasize("FmpBig_deployed")) return(0, datasize("FmpBig_deployed")) }
  object "FmpBig_deployed" {
    code {
      let op := calldataload(0)
      let v := calldataload(32)
      let r := 0
      switch op
      // direct untrusted store to FMP then load
      case 0 { mstore(0x40, v) r := mload(0x40) }
      // allocator-looking store (add(mload(0x40), v)) then load
      case 1 { mstore(0x40, add(mload(0x40), v)) r := mload(0x40) }
      // store big, use as offset
      case 2 { mstore(0x40, v) let p := mload(0x40) r := p }
      mstore(0, r)
      return(0, 32)
    }
  }
}
