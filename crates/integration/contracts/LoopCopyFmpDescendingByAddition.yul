/// solc's optimizer rewrites `sub(pointer, 0x20)` into `add(pointer, not(0x1f))`. A copy through a loop counter descending by that addition reaches the free memory pointer on a later iteration; reverts on a mismatch.
object "LoopCopyFmpDescendingByAddition" {
  code { datacopy(0, dataoffset("LoopCopyFmpDescendingByAddition_deployed"), datasize("LoopCopyFmpDescendingByAddition_deployed")) return(0, datasize("LoopCopyFmpDescendingByAddition_deployed")) }
  object "LoopCopyFmpDescendingByAddition_deployed" {
    code {
      mstore(0x40, 0x80)
      for { let pointer := 0x80 } gt(pointer, 0x20) { pointer := add(pointer, not(0x1f)) } { calldatacopy(pointer, 0, 0x20) }
      if iszero(eq(mload(0x40), calldataload(0))) { revert(0, 0) }
      return(0, 0)
    }
  }
}
