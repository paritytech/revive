/// ICE regression (newyork heap analysis): the word walk over a static memory range must not
/// overflow `u64` for an access at the top of the address space.
///
/// `add(mul(MAX_U256, 1), 0x41)` folds to `0x40`, so the second `mstore` writes `u64::MAX` over
/// the free-memory-pointer word. `mem_opt` forwards `mload(0x40)` to that literal, turning
/// `mstore(f2, _)` into a static store at `u64::MAX`, where the walk panicked with "attempt to
/// add with overflow" on valid Yul. Compiling this at all is the regression; the run also pins
/// the behaviour against EVM.
object "HeapRangeOverflowBug" {
  code { datacopy(0, dataoffset("HeapRangeOverflowBug_deployed"), datasize("HeapRangeOverflowBug_deployed")) return(0, datasize("HeapRangeOverflowBug_deployed")) }
  object "HeapRangeOverflowBug_deployed" {
    code {
      mstore(0x40, 0x80)
      mstore(add(mul(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1), 0x41),
             0x000000000000000000000000000000000000000000000000ffffffffffffffff)
      let fmp := mload(0x40)
      let f2 := mload(0x40)
      mstore(f2, 0xC0FFEE)
      let rb := mload(f2)
      mstore(0, fmp)
      mstore(32, rb)
      return(0, 64)
    }
  }
}
