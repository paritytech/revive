/// `add(p, not(0x5f))` moves the free memory pointer from 0x80 down to 0x20, so `sub(0x10, mload(0x40))` wraps to 2^256 - 0x10.
/// The call `g(4)` after the store makes the memory optimizer forget it, so only `FmpPropagation` forwards the pointer to the load;
/// `g(1)` to `g(3)` give `g` enough call sites that it is not inlined.
object "FmpWrapSubtract" {
  code { datacopy(0, dataoffset("FmpWrapSubtract_deployed"), datasize("FmpWrapSubtract_deployed")) return(0, datasize("FmpWrapSubtract_deployed")) }
  object "FmpWrapSubtract_deployed" {
    code {
      mstore(0x40, 0x80)
      g(1) g(2) g(3)
      let p := mload(0x40)
      mstore(0x40, add(p, not(0x5f)))
      g(4)
      mstore(0, sub(0x10, mload(0x40)))
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
