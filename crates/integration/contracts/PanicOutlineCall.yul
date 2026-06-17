object "PanicOutlineCall" {
  code { let s := datasize("PanicOutlineCall_deployed") codecopy(0, dataoffset("PanicOutlineCall_deployed"), s) return(0, s) }
  object "PanicOutlineCall_deployed" {
    code {
      // A canonical Panic(uint256) shape, but with an internal call between the selector store and
      // the revert. `inner` is recursive so it is never inlined; it reverts with distinct data before
      // the panic revert is reached. Collapsing this window into a single PanicRevert would drop the
      // call and revert with the wrong (panic) payload.
      mstore(0, 0x4e487b7100000000000000000000000000000000000000000000000000000000)
      inner(0)
      mstore(4, 0x11)
      revert(0, 0x24)
      function inner(n) {
        if n { inner(sub(n, 1)) }
        mstore(0x80, 0xdeadbeef)
        revert(0x80, 0x20)
      }
    }
  }
}
