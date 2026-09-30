/// `add(p, not(0x7f))` moves the free memory pointer from 0x80 down to 0, so `lt(mload(0x40), 1)` is 1.
/// The call `g(4)` after the store makes the memory optimizer forget it, so only `FmpPropagation` forwards
/// the pointer to the load; `g(1)` to `g(3)` give `g` enough call sites that it is not inlined.
object "FmpWrapCompare" {
  code { datacopy(0, dataoffset("FmpWrapCompare_deployed"), datasize("FmpWrapCompare_deployed")) return(0, datasize("FmpWrapCompare_deployed")) }
  object "FmpWrapCompare_deployed" {
    code {
      mstore(0x40, 0x80)
      g(1) g(2) g(3)
      let p := mload(0x40)
      mstore(0x40, add(p, not(0x7f)))
      g(4)
      mstore(0, lt(mload(0x40), 1))
      return(0, 32)
      function g(k) {
        sstore(k, add(k, 2))
        sstore(add(k, 3), mul(k, 5))
        sstore(add(k, 7), div(k, 11))
        sstore(add(k, 13), mod(k, 17))
      }
    }
  }
}
