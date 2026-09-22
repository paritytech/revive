/// Zero-length returns from offsets past the pointer width and the heap size.
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
