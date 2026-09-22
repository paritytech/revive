/// Regression (newyork mapping outlining): the outlined `__revive_mapping_sstore`
/// helper hashed its pre-image from a private alloca, while the keccak fusion in
/// `mem_opt` had already dead-eliminated the `mstore(0, key)` and `mstore(0x20, slot)`
/// staging stores on the premise that the helper reproduces them. Scratch `[0, 0x40)`
/// was therefore left stale after a fused mapping store, and `return(0, 0x40)`
/// returned zeros where EVM returns `key || slot`. Nine mapping stores reach the
/// outlining threshold; the last one leaves `key || 9` in scratch.
object "MappingSStoreScratch" {
  code { datacopy(0, dataoffset("MappingSStoreScratch_deployed"), datasize("MappingSStoreScratch_deployed")) return(0, datasize("MappingSStoreScratch_deployed")) }
  object "MappingSStoreScratch_deployed" {
    code {
      let key := calldataload(0)
      let value := calldataload(32)
      mstore(0, key) mstore(0x20, 1) sstore(keccak256(0, 0x40), value)
      mstore(0, key) mstore(0x20, 2) sstore(keccak256(0, 0x40), value)
      mstore(0, key) mstore(0x20, 3) sstore(keccak256(0, 0x40), value)
      mstore(0, key) mstore(0x20, 4) sstore(keccak256(0, 0x40), value)
      mstore(0, key) mstore(0x20, 5) sstore(keccak256(0, 0x40), value)
      mstore(0, key) mstore(0x20, 6) sstore(keccak256(0, 0x40), value)
      mstore(0, key) mstore(0x20, 7) sstore(keccak256(0, 0x40), value)
      mstore(0, key) mstore(0x20, 8) sstore(keccak256(0, 0x40), value)
      mstore(0, key) mstore(0x20, 9) sstore(keccak256(0, 0x40), value)
      return(0, 0x40)
    }
  }
}
