/// `add(p, not(0x5f))` moves the free memory pointer down to 0x20, so the copy to `mload(0x40)` overwrites it and `mload(0x40)` must return the copied calldata word.
object "FmpWrapCopy" {
  code { datacopy(0, dataoffset("FmpWrapCopy_deployed"), datasize("FmpWrapCopy_deployed")) return(0, datasize("FmpWrapCopy_deployed")) }
  object "FmpWrapCopy_deployed" {
    code {
      mstore(0x40, 0x80)
      g(1) g(2) g(3)
      let p := mload(0x40)
      mstore(0x40, add(p, not(0x5f)))
      g(4)
      let q := mload(0x40)
      mstore(0x40, q)
      c(0) c(1) c(2)
      mstore(0, mload(0x40))
      return(0, 32)
      function g(k) {
        sstore(k, add(k, 2))
        sstore(add(k, 3), mul(k, 5))
        sstore(add(k, 7), div(k, 11))
        sstore(add(k, 13), mod(k, 17))
      }
      function c(k) {
        if iszero(k) { calldatacopy(mload(0x40), 0, 0x40) }
        sstore(add(k, 100), add(k, 2))
        sstore(add(k, 103), mul(k, 5))
        sstore(add(k, 107), div(k, 11))
        sstore(add(k, 113), mod(k, 17))
      }
    }
  }
}
