/// Regression: a static store at `u64::MAX` through a corrupted free memory pointer crashed newyork.
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
