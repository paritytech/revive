object "FunctionCvReturnOnly" {
  code { let s := datasize("FunctionCvReturnOnly_deployed") codecopy(0, dataoffset("FunctionCvReturnOnly_deployed"), s) return(0, s) }
  object "FunctionCvReturnOnly_deployed" {
    code {
      // `tally` hands callvalue() back through its return variable on the fall-through path,
      // so the binding's only use is recorded in `Function::return_values` rather than in any
      // statement. Three callvalue() sites put the module over the outlined-callvalue
      // threshold, so codegen runs the dead-callvalue analysis; it must count return
      // variables as uses, otherwise the binding is skipped and the function hands back the
      // zero its return slot was seeded with. Two call sites and a body above the
      // always-inline size keep `tally` a real function instead of being inlined away.
      function tally(seed) -> r {
        sstore(10, seed)
        sstore(11, add(seed, 1))
        sstore(12, add(seed, 2))
        sstore(13, add(seed, 3))
        sstore(14, add(seed, 4))
        sstore(15, add(seed, 5))
        sstore(16, add(seed, 6))
        sstore(17, add(seed, 7))
        r := callvalue()
      }
      let sel := calldataload(0)
      switch sel
      case 1 { sstore(0, tally(1)) }
      case 2 { sstore(0, tally(2)) }
      case 3 { sstore(0, add(callvalue(), calldatasize())) }
      default { sstore(0, add(callvalue(), 0x20)) }
    }
  }
}
