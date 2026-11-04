/// Returns scratch `[0, 0x40)` after nine fused mapping stores.
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
