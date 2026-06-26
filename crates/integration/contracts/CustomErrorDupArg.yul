object "CustomErrorDupArg" {
  code { let s := datasize("CustomErrorDupArg_deployed") codecopy(0, dataoffset("CustomErrorDupArg_deployed"), s) return(0, s) }
  object "CustomErrorDupArg_deployed" {
    code {
      // Custom-error revert shape (selector + one uint256 argument), but the argument word is written
      // twice. EVM last-write-wins, so the revert argument is 0xbbbb. A reverse scan that keeps the
      // first (earliest) match would collapse this to an error carrying 0xaaaa.
      mstore(0, 0x1234567800000000000000000000000000000000000000000000000000000000)
      mstore(4, 0xaaaa)
      mstore(4, 0xbbbb)
      revert(0, 0x24)
    }
  }
}
