object "SwitchCvFallthrough" {
  code { let s := datasize("SwitchCvFallthrough_deployed") codecopy(0, dataoffset("SwitchCvFallthrough_deployed"), s) return(0, s) }
  object "SwitchCvFallthrough_deployed" {
    code {
      let sel := calldataload(0)
      switch sel
      // Each selector is non-payable: it reverts if value was sent.
      case 1 { let cv := callvalue() if cv { revert(0, 0) } sstore(0, 0x11) return(0, 0) }
      case 2 { let cv := callvalue() if cv { revert(0, 0) } sstore(0, 0x22) return(0, 0) }
      // There is NO default. A no-match selector must fall through to here. Hoisting the cases'
      // callvalue check above the switch would make this fall-through path revert on a nonzero
      // value, even though it never had a callvalue check of its own.
      sstore(0, 0xff)
      return(0, 0)
    }
  }
}
