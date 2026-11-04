/// Returns scratch `[0, 0x40)` written between a fused mapping load's hash and its `sload`.
object "MappingSLoadScratchOrder" {
  code { datacopy(0, dataoffset("MappingSLoadScratchOrder_deployed"), datasize("MappingSLoadScratchOrder_deployed")) return(0, datasize("MappingSLoadScratchOrder_deployed")) }
  object "MappingSLoadScratchOrder_deployed" {
    code {
      let key := calldataload(0)
      let value := calldataload(32)
      mstore(0, key) mstore(0x20, 1) sstore(1, sload(keccak256(0, 0x40)))
      mstore(0, key) mstore(0x20, 2) sstore(2, sload(keccak256(0, 0x40)))
      mstore(0, key) mstore(0x20, 3) sstore(3, sload(keccak256(0, 0x40)))
      mstore(0, key) mstore(0x20, 4) sstore(4, sload(keccak256(0, 0x40)))
      mstore(0, key) mstore(0x20, 5) sstore(5, sload(keccak256(0, 0x40)))
      mstore(0, key) mstore(0x20, 6) sstore(6, sload(keccak256(0, 0x40)))
      mstore(0, key) mstore(0x20, 7) sstore(7, sload(keccak256(0, 0x40)))
      mstore(0, key) mstore(0x20, 8) sstore(8, sload(keccak256(0, 0x40)))
      mstore(0, key) mstore(0x20, 9) sstore(9, sload(keccak256(0, 0x40)))
      mstore(0, key) mstore(0x20, 10)
      let hash := keccak256(0, 0x40)
      mstore(0, value) mstore(0x20, value)
      sstore(10, sload(hash))
      return(0, 0x40)
    }
  }
}
