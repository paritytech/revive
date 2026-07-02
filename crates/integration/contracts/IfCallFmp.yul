object "IfCallFmp" {
  code { let s := datasize("IfCallFmp_deployed") codecopy(0, dataoffset("IfCallFmp_deployed"), s) return(0, s) }
  object "IfCallFmp_deployed" {
    code {
      // Known free-memory pointer, tracked as a constant by FMP propagation.
      mstore(0x40, 0x80)
      let cnt := calldataload(0)
      // The branch calls an internal allocator that bumps the FMP. `allocate` is recursive, so it is
      // never inlined: the `mstore(0x40, ...)` lives inside the callee, not inside this `if` body.
      // Region FMP invalidation must therefore consult `fmp_writers` (the call), not just look for a
      // direct FMP store in the branch — otherwise the post-`if` `mload(0x40)` is forwarded the stale
      // pre-branch constant 0x80 even when `cnt > 0` bumped the pointer.
      if cnt { allocate(cnt) }
      let p := mload(0x40)
      mstore(0x200, p)
      return(0x200, 0x20)
      function allocate(n) {
        if n {
          let f := mload(0x40)
          mstore(0x40, add(f, 0x20))
          allocate(sub(n, 1))
        }
      }
    }
  }
}
