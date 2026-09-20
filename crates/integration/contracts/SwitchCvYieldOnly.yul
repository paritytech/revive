object "SwitchCvYieldOnly" {
  code { let s := datasize("SwitchCvYieldOnly_deployed") codecopy(0, dataoffset("SwitchCvYieldOnly_deployed"), s) return(0, s) }
  object "SwitchCvYieldOnly_deployed" {
    code {
      let sel := calldataload(0)
      // Three callvalue() sites put the module over the outlined-callvalue threshold, so
      // codegen runs the dead-callvalue analysis. Case 1 binds callvalue() and its only use
      // is the region yield carrying the switch result into `r`. The other branches evaluate
      // their second operand first, so no branch-leading callvalue hoist merges the bindings
      // and the yield-only binding survives to codegen. The analysis must count region yields
      // as uses, otherwise the binding is skipped and the yield references an undefined value.
      let r := 0
      switch sel
      case 1 { r := callvalue() }
      case 2 { r := add(callvalue(), calldatasize()) }
      default { r := add(callvalue(), 0x20) }
      sstore(0, r)
    }
  }
}
