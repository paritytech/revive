object "CallvalueRevertData" {
  code { let s := datasize("CallvalueRevertData_deployed") codecopy(0, dataoffset("CallvalueRevertData_deployed"), s) return(0, s) }
  object "CallvalueRevertData_deployed" {
    code {
      let sel := calldataload(0)
      // One callvalue read per case (separate regions, so CSE keeps them distinct): three reads
      // enable the callvalue-check outline.
      switch sel
      case 1 { if callvalue() { revert(0, 0) } sstore(0, 0x11) }
      case 2 { if callvalue() { revert(0, 0) } sstore(0, 0x22) }
      case 3 {
        // A data-carrying revert whose operands are bound in the enclosing (case) scope. The outline
        // must not treat this as an empty revert and drop the returned memory[off, off+len).
        mstore(0x80, 0xdeadbeef)
        let off := calldataload(32)
        let len := calldataload(64)
        if callvalue() { revert(off, len) }
        sstore(0, 0x33)
      }
      default { sstore(0, 0xff) }
    }
  }
}
