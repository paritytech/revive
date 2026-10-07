/// Stores the complement of what the deploy code reads through calldatacopy, calldataload and calldatasize.
object "DeployCallData" {
  code {
    mstore(0, not(0))
    calldatacopy(0, 0, 32)
    sstore(0, not(mload(0)))
    sstore(1, not(calldataload(0)))
    sstore(2, not(calldatasize()))
    datacopy(0, dataoffset("DeployCallData_deployed"), datasize("DeployCallData_deployed"))
    return(0, datasize("DeployCallData_deployed"))
  }
  object "DeployCallData_deployed" {
    code { return(0, 0) }
  }
}
