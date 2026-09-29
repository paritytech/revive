/// `add(mload(0x40), not(0xff))` moves the zero free memory pointer to 2^256 - 0x100, so `mload(0x40)` must return that word.
object "FmpWrapUntracked" {
  code { datacopy(0, dataoffset("FmpWrapUntracked_deployed"), datasize("FmpWrapUntracked_deployed")) return(0, datasize("FmpWrapUntracked_deployed")) }
  object "FmpWrapUntracked_deployed" {
    code {
      mstore(0x40, add(mload(0x40), not(0xff)))
      g(1) g(2) g(3)
      mstore(0, mload(0x40))
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
