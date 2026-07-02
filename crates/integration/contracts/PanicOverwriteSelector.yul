object "PanicOverwriteSelector" {
  code { let s := datasize("PanicOverwriteSelector_deployed") codecopy(0, dataoffset("PanicOverwriteSelector_deployed"), s) return(0, s) }
  object "PanicOverwriteSelector_deployed" {
    code {
      // Canonical panic selector + code, but the selector word is then overwritten before the
      // revert. Last-write-wins means the EVM revert data starts with 0xdeadbeef, not the panic
      // selector — so this must NOT be collapsed into a canonical Panic(0x11).
      mstore(0, 0x4e487b7100000000000000000000000000000000000000000000000000000000)
      mstore(4, 0x11)
      mstore(0, 0xdeadbeef)
      revert(0, 0x24)
    }
  }
}
