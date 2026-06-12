object "ForLoopFmp" {
  code { let s := datasize("ForLoopFmp_deployed") codecopy(0, dataoffset("ForLoopFmp_deployed"), s) return(0, s) }
  object "ForLoopFmp_deployed" {
    code {
      // Establish a known free-memory pointer so FMP propagation tracks it as a constant.
      mstore(0x40, 0x80)
      // Dynamic trip count from calldata keeps the loop from being unrolled (which would hide the
      // per-iteration re-read of the FMP).
      let n := calldataload(0)
      // Each iteration allocates a fresh 32-byte slot at the current FMP and bumps the pointer.
      // The loop-top `mload(0x40)` must observe the bump from the previous iteration; if FMP
      // propagation rewrites it to the pre-loop constant, every iteration aliases the first slot.
      for { let i := 0 } lt(i, n) { i := add(i, 1) } {
        let p := mload(0x40)
        mstore(p, add(i, 1))
        mstore(0x40, add(p, 0x20))
      }
      // Read back the first three allocated slots. Correct: [1, 2, 3] for n >= 3.
      // With the aliasing bug: [n, 0, 0].
      mstore(0x200, mload(0x80))
      mstore(0x220, mload(0xa0))
      mstore(0x240, mload(0xc0))
      return(0x200, 0x60)
    }
  }
}
