/// Exits with runtime offsets and lengths, inline and across function calls.
object "RuntimeLengthExit" {
  code { datacopy(0, dataoffset("RuntimeLengthExit_deployed"), datasize("RuntimeLengthExit_deployed")) return(0, datasize("RuntimeLengthExit_deployed")) }
  object "RuntimeLengthExit_deployed" {
    code {
      function return_from(offset, length, depth) {
        if depth { return_from(offset, length, sub(depth, 1)) }
        return(offset, length)
      }
      function revert_from(offset, length, depth) {
        if depth { revert_from(offset, length, sub(depth, 1)) }
        revert(offset, length)
      }
      let offset := calldataload(32)
      let length := calldataload(64)
      switch calldataload(0)
      case 0 { return(offset, length) }
      case 1 { revert(offset, length) }
      case 2 { return_from(offset, length, calldataload(96)) }
      default { revert_from(offset, length, calldataload(96)) }
    }
  }
}
