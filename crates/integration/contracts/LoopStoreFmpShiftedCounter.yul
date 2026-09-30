/// A store at `mul(index, 0x20)`, which newyork rewrites into `shl(5, index)`, reaches the free memory pointer from `index = 2`; reverts on a mismatch.
object "LoopStoreFmpShiftedCounter" {
  code { datacopy(0, dataoffset("LoopStoreFmpShiftedCounter_deployed"), datasize("LoopStoreFmpShiftedCounter_deployed")) return(0, datasize("LoopStoreFmpShiftedCounter_deployed")) }
  object "LoopStoreFmpShiftedCounter_deployed" {
    code {
      mstore(0x40, 0x80)
      let value := calldataload(0)
      for { let index := 2 } lt(index, 3) { index := add(index, 1) } { mstore(mul(index, 0x20), value) }
      if iszero(eq(mload(0x40), value)) { revert(0, 0) }
      return(0, 0)
    }
  }
}
