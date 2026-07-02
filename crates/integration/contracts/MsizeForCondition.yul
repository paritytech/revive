object "MsizeForCondition" {
  code { let s := datasize("MsizeForCondition_deployed") codecopy(0, dataoffset("MsizeForCondition_deployed"), s) return(0, s) }
  object "MsizeForCondition_deployed" {
    code {
      let x := calldataload(0)
      // Touch memory at 0x80 so the EVM free-memory watermark (msize) is a
      // nonzero 0xA0, regardless of `x`.
      mstore(0x80, x)
      let ran := 0
      // A *bare* `msize()` for-loop condition: this is the only expression
      // position the IR does not materialize into a preceding `let`. The body
      // runs once iff `msize()` is truthy. If a native store fails to update
      // the heap-size watermark (because `has_msize` missed this position),
      // `msize()` reads stale 0, the body is skipped, and `ran` diverges.
      for {} msize() {} {
        ran := add(ran, 1)
        break
      }
      mstore(0x400, ran)
      return(0x400, 32)
    }
  }
}
