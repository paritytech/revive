/// `shl(shift, 1)` with the forwarded shift 5 stores at 0x20, so the store to 10 only overwrites part of it.
object "ShlOffsetDeadStore" {
  code { datacopy(0, dataoffset("ShlOffsetDeadStore_deployed"), datasize("ShlOffsetDeadStore_deployed")) return(0, datasize("ShlOffsetDeadStore_deployed")) }
  object "ShlOffsetDeadStore_deployed" {
    code {
      mstore(0, 5)
      let shift := mload(0)
      mstore(shl(shift, 1), calldataload(0))
      mstore(10, 0)
      return(32, 32)
    }
  }
}
