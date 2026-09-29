/// `f(0x40)` stores 1 at an offset it computes, the free memory pointer word, so `mload(0x40)` must return 1, not the computed pointer 5.
object "FmpComputedCallStore" {
  code { datacopy(0, dataoffset("FmpComputedCallStore_deployed"), datasize("FmpComputedCallStore_deployed")) return(0, datasize("FmpComputedCallStore_deployed")) }
  object "FmpComputedCallStore_deployed" {
    code {
      mstore(0, 5)
      let v := mload(0)
      mstore(0x40, add(eq(not(0x1f), sar(v, not(0x7e))), 5))
      pop(f(0x20)) pop(f(0x40)) pop(f(0x60))
      mstore(0, mload(0x40))
      return(0, 0x20)
      function f(a0) -> x {
        mstore(and(a0, 0x3df), 1)
        sstore(a0, add(a0, 2))
        sstore(add(a0, 3), mul(a0, 5))
        sstore(add(a0, 7), div(a0, 11))
        x := a0
      }
    }
  }
}
