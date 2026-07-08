/// Soundness PoC (newyork dead-store elimination vs `msize`): a store whose
/// memory expansion an intervening `msize()` observes must not be eliminated as
/// dead by a later overwrite.
///
/// `mstore(288, 1)` writes `[288, 320)`, expanding active memory to `0x140`.
/// `msize()` between the two stores must observe that expansion (`0x140`). The
/// dead-store pass dropped the first store because `mstore(288, 2)` overwrites
/// the same offset — but that moved the expansion to *after* the `msize()`, so
/// `msize()` read the smaller (unexpanded) value `0`. EVM (and the stock
/// pipeline) return `0x140`. The fix clears the dead-store candidate set when an
/// `msize()` is evaluated, since it observes the cumulative memory expansion of
/// all prior stores.
object "MsizeDseBug" {
  code { datacopy(0, dataoffset("MsizeDseBug_deployed"), datasize("MsizeDseBug_deployed")) return(0, datasize("MsizeDseBug_deployed")) }
  object "MsizeDseBug_deployed" {
    code {
      mstore(288, 1)
      let r := msize()
      mstore(288, 2)
      mstore(0, r)
      return(0, 32)
    }
  }
}
