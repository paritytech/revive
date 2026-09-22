/// Regression: a `return` of zero length must succeed with empty data for any offset,
/// because EVM performs no memory expansion for it. The exit runtime function truncated
/// the offset to the pointer width before looking at the length, so an offset at or
/// above `2^32` trapped, and the `--newyork` checked exit also trapped on an offset past
/// the heap size. Case 1 uses a literal offset of `not(0)`, case 2 a literal past the
/// heap size, case 3 a calldata supplied offset.
object "ZeroLengthExit" {
  code { datacopy(0, dataoffset("ZeroLengthExit_deployed"), datasize("ZeroLengthExit_deployed")) return(0, datasize("ZeroLengthExit_deployed")) }
  object "ZeroLengthExit_deployed" {
    code {
      switch calldataload(0)
      case 1 { return(not(0), 0) }
      case 2 { return(0x100000, 0) }
      case 3 { return(calldataload(32), 0) }
      default { revert(not(0), 0) }
    }
  }
}
