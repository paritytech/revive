/// Soundness PoC (newyork type inference): a value stored to the free-memory-pointer
/// slot `0x40` must not be narrowed to a 64-bit pointer width when it is also
/// observable at full width elsewhere.
///
/// `f0`'s parameter `p0` is stored to the FMP slot (`mstore(64, p0)`) — which
/// inference treated as a bounded pointer and narrowed to i64 — but `p0` is also
/// stored full-width at offset 8 and read back by `mload(24)`. The i64 narrowing
/// of the parameter inserts a call-boundary guard that traps (consumes all gas)
/// when a caller passes an argument `>= 2^64`. The first call passes `b`
/// (`calldataload(32)`, a full 256-bit word), so newyork ran out of gas where EVM
/// (and the stock pipeline) execute successfully. The fix records the FMP-stored
/// value as an ordinary full-width memory value.
object "FnNativeFmpBug" {
  code { datacopy(0, dataoffset("FnNativeFmpBug_deployed"), datasize("FnNativeFmpBug_deployed")) return(0, datasize("FnNativeFmpBug_deployed")) }
  object "FnNativeFmpBug_deployed" {
    code {
      let a := calldataload(0) let b := calldataload(32) let d := calldataload(96) let r := 0
      function f0(p0, p1) -> ret {
        mstore(8, p0)
        mstore(64, p0)
        mstore8(8, p1)
        ret := mload(24)
      }
      r := add(mul(r, 0x100000001b3), f0(b, a))
      r := add(mul(r, 0x100000001b3), f0(2719014241053524200, d))
      r := add(mul(r, 0x100000001b3), f0(391540675041506305, b))
      mstore(0, r)
      return(0, 32)
    }
  }
}
