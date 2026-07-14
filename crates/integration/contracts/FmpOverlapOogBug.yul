/// Soundness PoC (newyork FMP range proof): an unaligned `mstore` that overlaps
/// the free-memory-pointer word `[0x40, 0x60)` corrupts the pointer with
/// arbitrary bytes, yet newyork kept its `FMP < heap_size` range proof, so a
/// subsequent store through the corrupted pointer silently succeeded instead of
/// running out of gas as EVM does on the unbounded memory expansion.
///
/// `mstore(56, not(0))` covers `[0x38, 0x58)`, overwriting the FMP word
/// `[0x40, 0x60)` with `0xff...`. `mload(0x40)` then reads the corrupted (huge)
/// pointer, and `mstore(mload(0x40), 0x42)` expands memory unboundedly — EVM
/// (and the stock pipeline) run out of gas. The fix flags `fmp_could_be_unbounded`
/// when such an overlap store is *observed* by a later `mload(0x40)`, disabling
/// the range proof so the corrupted pointer traps.
object "FmpOverlapOogBug" {
  code { datacopy(0, dataoffset("FmpOverlapOogBug_deployed"), datasize("FmpOverlapOogBug_deployed")) return(0, datasize("FmpOverlapOogBug_deployed")) }
  object "FmpOverlapOogBug_deployed" {
    code {
      mstore(0x40, 0x80)
      mstore(56, not(0))
      mstore(mload(0x40), 0x42)
      mstore(0, 0)
      return(0, 32)
    }
  }
}
