object "ShiftParamNarrow" {
  code { datacopy(0, dataoffset("ShiftParamNarrow_deployed"), datasize("ShiftParamNarrow_deployed")) return(0, datasize("ShiftParamNarrow_deployed")) }
  object "ShiftParamNarrow_deployed" {
    code {
      let opx := calldataload(0)
      let shamt := calldataload(32)
      let xv := calldataload(64)
      let r0 := f(opx, shamt, xv)
      let r1 := f(opx, shamt, xv)
      let r2 := f(opx, shamt, xv)
      let r3 := f(opx, shamt, xv)
      mstore(0x200, r0)
      mstore(0x220, add(r1, add(r2, r3)))
      return(0x200, 64)
      function f(op, s, v) -> r {
        switch op
        case 0 { r := mload(s) }                 // narrows s to i64
        case 1 { r := shl(s, v) }
        case 2 { r := shr(s, v) }
        case 3 { r := sar(s, v) }
        case 4 {
          // bulk to push f over the inline threshold (keep f a real function)
          mstore(0x300, s) mstore(0x320, v) mstore(0x340, op)
          r := keccak256(0x300, 0x60)
          r := add(r, keccak256(0x300, 0x40))
          r := add(r, keccak256(0x300, 0x20))
        }
        default { r := s }
      }
    }
  }
}
