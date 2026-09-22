/// Regression (newyork FMP range proof): an `mcopy` destination or an external call's
/// return range covering the free-memory-pointer word `[0x40, 0x60)` replaces the
/// pointer with arbitrary bytes, but neither statement flagged the pointer as possibly
/// unbounded. The `FMP < heap_size` range proof then truncated the clobbered
/// `mload(0x40)` to the heap size width instead of returning the copied word.
///
/// Case 1 copies the calldata word at 32 onto a calldata-supplied destination (`0x40`
/// in the test). Case 2 calls the contract itself, which answers with `not(0)`, and
/// returns that word into `0x40`. Both cases then read the pointer back.
object "CopyFmpBug" {
  code { datacopy(0, dataoffset("CopyFmpBug_deployed"), datasize("CopyFmpBug_deployed")) return(0, datasize("CopyFmpBug_deployed")) }
  object "CopyFmpBug_deployed" {
    code {
      switch calldataload(0)
      case 1 {
        mstore(0x80, calldataload(32))
        mcopy(and(calldataload(64), 0xff), 0x80, 0x20)
      }
      case 2 {
        if iszero(staticcall(gas(), address(), 0, 0, 0x40, 0x20)) { revert(0, 0) }
      }
      default {
        mstore(0, not(0))
        return(0, 32)
      }
      mstore(0, mload(0x40))
      return(0, 32)
    }
  }
}
