object "PanicOutlineYield" {
  code { let s := datasize("PanicOutlineYield_deployed") codecopy(0, dataoffset("PanicOutlineYield_deployed"), s) return(0, s) }
  object "PanicOutlineYield_deployed" {
    code {
      let cond := calldataload(0)
      let r := 0
      // `r` is reassigned inside the branch, between the panic selector store and the revert, and is
      // read after the branch — so the branch region yields that in-window definition of `r`.
      // Collapsing the window into a PanicRevert drops the definition; without a zero-binding rescue
      // the surviving yield references an undefined value and the SSA validator panics.
      if cond {
        mstore(0, 0x4e487b7100000000000000000000000000000000000000000000000000000000)
        r := 7
        mstore(4, 0x11)
        revert(0, 0x24)
      }
      sstore(0, r)
      return(0, 0)
    }
  }
}
