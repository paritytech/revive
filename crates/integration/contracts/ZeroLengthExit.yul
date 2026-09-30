/// Zero-length exits from constant and runtime offsets, inline and across function calls.
object "ZeroLengthExit" {
  code { datacopy(0, dataoffset("ZeroLengthExit_deployed"), datasize("ZeroLengthExit_deployed")) return(0, datasize("ZeroLengthExit_deployed")) }
  object "ZeroLengthExit_deployed" {
    code {
      function return_empty(offset, depth) {
        if depth { return_empty(offset, sub(depth, 1)) }
        return(offset, 0)
      }
      function revert_empty(offset, depth) {
        if depth { revert_empty(offset, sub(depth, 1)) }
        revert(offset, 0)
      }
      let offset := calldataload(32)
      switch calldataload(0)
      case 0 { return(offset, 0) }
      case 1 { revert(offset, 0) }
      case 2 { return_empty(offset, calldataload(64)) }
      case 3 { revert_empty(offset, calldataload(64)) }
      case 4 { return(not(0), 0) }
      case 5 { revert(not(0), 0) }
      case 6 { return(0x20000, 0) }
      default { revert(0x20000, 0) }
    }
  }
}
