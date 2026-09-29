/// solc's optimizer rewrites `sub(pointer, 0x20)` into `add(pointer, not(0x1f))`. A store through a loop counter descending by that addition reaches the free memory pointer on a later iteration; reverts on a mismatch.
object "LoopStoreFmpDescendingByAddition" {
  code { datacopy(0, dataoffset("LoopStoreFmpDescendingByAddition_deployed"), datasize("LoopStoreFmpDescendingByAddition_deployed")) return(0, datasize("LoopStoreFmpDescendingByAddition_deployed")) }
  object "LoopStoreFmpDescendingByAddition_deployed" {
    code {
      mstore(0x40, 0x80)
      let value := calldataload(0)
      for { let pointer := 0x80 } gt(pointer, 0x20) { pointer := add(pointer, not(0x1f)) } { mstore(pointer, value) }
      if iszero(eq(mload(0x40), value)) { revert(0, 0) }
      return(0, 0)
    }
  }
}
