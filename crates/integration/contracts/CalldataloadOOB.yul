object "CalldataloadOOB" {
  code { datacopy(0, dataoffset("CalldataloadOOB_deployed"), datasize("CalldataloadOOB_deployed")) return(0, datasize("CalldataloadOOB_deployed")) }
  object "CalldataloadOOB_deployed" {
    code {
      let off := calldataload(0)        // the offset to test (from calldata[0])
      let x := calldataload(off)        // EVM: zero-pads beyond calldatasize
      mstore(0, x)
      return(0, 32)
    }
  }
}
