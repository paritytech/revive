object "SignedCmpNarrow" {
  code { datacopy(0, dataoffset("SignedCmpNarrow_deployed"), datasize("SignedCmpNarrow_deployed")) return(0, datasize("SignedCmpNarrow_deployed")) }
  object "SignedCmpNarrow_deployed" {
    code {
      let op := calldataload(0)
      let a := calldataload(32)
      let b := calldataload(64)
      let r := 0
      switch op
      // i1 operands: sgt(0, 1) must be 0 (EVM), not 1
      case 0 { r := sgt(eq(a, b), gt(a, b)) }
      case 1 { r := slt(gt(a, b), eq(a, b)) }
      // i8 operands with top bit set: sgt(0xC8, 0x05) must be 1 (both positive)
      case 2 { r := sgt(and(a, 0xFF), and(b, 0xFF)) }
      case 3 { r := slt(and(a, 0xFF), and(b, 0xFF)) }
      mstore(0, r) return(0, 32)
    }
  }
}
