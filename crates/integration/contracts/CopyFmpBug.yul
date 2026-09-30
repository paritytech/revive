/// An `mcopy` onto a calldata supplied destination and a `staticcall` returning into `0x40`.
object "CopyFmpBug" {
  code { datacopy(0, dataoffset("CopyFmpBug_deployed"), datasize("CopyFmpBug_deployed")) return(0, datasize("CopyFmpBug_deployed")) }
  object "CopyFmpBug_deployed" {
    code {
      switch calldataload(0)
      case 1 {
        mstore(0x80, calldataload(32))
        mcopy(and(calldataload(64), 0xff), 0x80, 0x20)
      }
      case 2 {
        if iszero(staticcall(gas(), address(), 0, 0, 0x40, 0x20)) { revert(0, 0) }
      }
      default {
        mstore(0, not(0))
        return(0, 32)
      }
      mstore(0, mload(0x40))
      return(0, 32)
    }
  }
}
