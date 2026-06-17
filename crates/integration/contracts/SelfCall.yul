object "SelfCall" {
  code { datacopy(0, dataoffset("SelfCall_deployed"), datasize("SelfCall_deployed")) return(0, datasize("SelfCall_deployed")) }
  object "SelfCall_deployed" {
    code {
      let flag := calldataload(0)
      let value := calldataload(32)
      switch flag
      // leaf: echo the value back as return data
      case 0 {
        mstore(0, value)
        return(0, 32)
      }
      // recurse: build [0, value] calldata, self-call, return the returndata word
      default {
        mstore(0x80, 0)
        mstore(0xA0, value)
        let ok := call(gas(), address(), 0, 0x80, 64, 0x100, 32)
        // mix in returndatasize and a returndatacopy round-trip
        returndatacopy(0x200, 0, returndatasize())
        let r := mload(0x100)
        r := xor(r, mload(0x200))
        r := add(r, ok)
        mstore(0, r)
        return(0, 32)
      }
    }
  }
}
