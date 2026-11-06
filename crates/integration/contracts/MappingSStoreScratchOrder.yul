/// Returns scratch `[0, 0x40)` written between a fused mapping store's hash and its `sstore`.
object "MappingSStoreScratchOrder" {
  code { datacopy(0, dataoffset("MappingSStoreScratchOrder_deployed"), datasize("MappingSStoreScratchOrder_deployed")) return(0, datasize("MappingSStoreScratchOrder_deployed")) }
  object "MappingSStoreScratchOrder_deployed" {
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
      mstore(0, key) mstore(0x20, 10)
      let hash := keccak256(0, 0x40)
      mstore(0, value) mstore(0x20, value)
      sstore(hash, value)
      return(0, 0x40)
    }
  }
}
