/// Force scrutinee to i64 via an explicit 64-bit mask.
object "SwitchWideLabel" {
  code { datacopy(0, dataoffset("SwitchWideLabel_deployed"), datasize("SwitchWideLabel_deployed")) return(0, datasize("SwitchWideLabel_deployed")) }
  object "SwitchWideLabel_deployed" {
    code {
      let x := and(calldataload(0), 0xffffffffffffffff)
      switch x
      case 0 { mstore(0, 0xAA) return(0, 32) }
      case 0x10000000000000000 { mstore(0, 0xBB) return(0, 32) }
      default { mstore(0, 0xCC) return(0, 32) }
    }
  }
}
