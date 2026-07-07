/// Soundness PoC (newyork FMP range proof): a `calldatacopy` whose *dynamic*
/// destination can land on the free-memory-pointer word `[0x40, 0x60)` corrupts
/// the FMP, but was not detected as doing so.
///
/// `calldatacopy(and(b, 0xFF), 0, 23)` copies 23 bytes to a destination that
/// ranges over `[0, 0xFF]`; with `b`'s low byte `0x40` it overwrites the FMP
/// word. A subsequent `mload(0x40)` reads the corrupted pointer, but newyork kept
/// its `FMP < heap_size` range proof (only *static* copy destinations flagged
/// `fmp_could_be_unbounded`) and truncated the value to `0`. EVM (and the stock
/// pipeline) return the copied bytes: 23 × 0x11 then 9 × 0x00. The fix flags a
/// dynamic copy destination that is not provably free-pointer-relative
/// (`add(mload(0x40), k)`, which is `>= 0x80`).
object "CalldatacopyFmpDynBug" {
  code { datacopy(0, dataoffset("CalldatacopyFmpDynBug_deployed"), datasize("CalldatacopyFmpDynBug_deployed")) return(0, datasize("CalldatacopyFmpDynBug_deployed")) }
  object "CalldatacopyFmpDynBug_deployed" {
    code {
      let b := calldataload(32)
      calldatacopy(and(b, 0xFF), 0, 23)
      mstore(0, mload(64))
      return(0, 32)
    }
  }
}
