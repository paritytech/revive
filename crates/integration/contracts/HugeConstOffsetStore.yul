object "HugeConstOffsetStore" {
  code { let s := datasize("HugeConstOffsetStore_deployed") codecopy(0, dataoffset("HugeConstOffsetStore_deployed"), s) return(0, s) }
  object "HugeConstOffsetStore_deployed" {
    code {
      let x := calldataload(0)
      // A dynamic store keeps the object out of all-native mode, so the huge CONSTANT-offset store
      // below takes the inline (unchecked) path rather than the bounds-checked AllNative path.
      mstore(calldataload(32), 7)
      // Huge constant offset: far past the fixed heap. EVM charges astronomical memory-expansion gas
      // and runs out of gas; the inline unchecked GEP would instead write out of the heap global. It
      // must trap (out-of-gas), not corrupt memory and fall through to the sstore.
      mstore(0xFFFFFFF0, x)
      sstore(0, 1)
      return(0, 0)
    }
  }
}
