object "UnreachableUseAfterPanic" {
  code {
    datacopy(0, dataoffset("UnreachableUseAfterPanic_deployed"), datasize("UnreachableUseAfterPanic_deployed"))
    return(0, datasize("UnreachableUseAfterPanic_deployed"))
  }
  object "UnreachableUseAfterPanic_deployed" {
    code {
      for { let i := 0 } lt(i, calldataload(0)) { i := add(i, 1) } {
        mstore(0, 0x4e487b7100000000000000000000000000000000000000000000000000000000)
        let t := add(i, 7)
        mstore(4, 0x11)
        revert(0, 0x24)
        sstore(0, t)
      }
    }
  }
}
