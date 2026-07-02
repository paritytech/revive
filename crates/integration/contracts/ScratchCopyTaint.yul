/// Register 0x20 (< 0x60, exempt from dynamic-escape guard) as an aligned native candidate.
object "ScratchCopyTaint" {
  code { datacopy(0, dataoffset("ScratchCopyTaint_deployed"), datasize("ScratchCopyTaint_deployed")) return(0, datasize("ScratchCopyTaint_deployed")) }
  object "ScratchCopyTaint_deployed" {
    code {
      mstore(0x20, 0)
      // Overwrite scratch [0x00,0x40) with big-endian calldata. Taints only word_align(0)=0.
      calldatacopy(0, 0, 64)
      // Read 0x20: if native-LE, byte-reverses the BE calldata word.
      let b := mload(0x20)
      mstore(0x80, b)
      return(0x80, 32)
    }
  }
}
