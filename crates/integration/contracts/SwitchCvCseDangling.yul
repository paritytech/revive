object "SwitchCvCseDangling" {
  code { let s := datasize("SwitchCvCseDangling_deployed") codecopy(0, dataoffset("SwitchCvCseDangling_deployed"), s) return(0, s) }
  object "SwitchCvCseDangling_deployed" {
    code {
      let sel := calldataload(0)
      // Every branch (cases + default) starts with an equivalent non-payable callvalue check, so the
      // check is fully hoisted above the switch and dropped from each branch. The cases also read
      // `callvalue()` a SECOND time; environment CSE rewrites that read to reuse the first binding.
      // Hoisting must redirect that reuse to the hoisted binding, otherwise it references the
      // dropped definition and trips the SSA validator.
      switch sel
      case 1 { let cv := callvalue() if cv { revert(0, 0) } let x := callvalue() sstore(0, add(x, 0x100)) }
      case 2 { let cv := callvalue() if cv { revert(0, 0) } let y := callvalue() sstore(0, add(y, 0x200)) }
      default { let cv := callvalue() if cv { revert(0, 0) } sstore(0, 0xdd) }
    }
  }
}
