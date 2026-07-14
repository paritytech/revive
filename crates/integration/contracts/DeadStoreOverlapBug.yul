/// Soundness PoC (newyork dead-store elimination): a store read back by an
/// intervening *unaligned overlapping* `mload` must not be eliminated as dead.
///
/// `mem_opt` marked a pending store read only when a later `mload` used the
/// *exact same* static offset. Here `mstore(1, PAT)` writes `[1, 33)`,
/// `mload(8)` reads `[8, 40)` — overlapping `[1, 33)` but at a different offset,
/// so the store stayed a dead-store candidate — and the second `mstore(1, PAT)`
/// eliminated it. `r := mload(8)` then read zeroed memory. EVM (and the stock
/// pipeline) read the stored bytes: bytes 7..31 of PAT followed by 7 zeros.
/// The fix marks every pending store whose 32-byte range the load overlaps as
/// read.
object "DeadStoreOverlapBug" {
  code { datacopy(0, dataoffset("DeadStoreOverlapBug_deployed"), datasize("DeadStoreOverlapBug_deployed")) return(0, datasize("DeadStoreOverlapBug_deployed")) }
  object "DeadStoreOverlapBug_deployed" {
    code {
      mstore(1, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)
      let r := mload(8)
      mstore(1, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)
      mstore(0, r)
      return(0, 32)
    }
  }
}
