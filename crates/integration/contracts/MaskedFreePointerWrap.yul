/// A copy destination of `add(mload(0x40), k)` masked to its low 64 bits wraps modulo 2^64.
/// With the free memory pointer at `0x80`, `k = 2^64 - 0x40` makes the destination `0x40`, so
/// the copy overwrites the free memory pointer with a full word of calldata. The following
/// `mload(0x40)` must return that full word rather than a value truncated to the heap width.
/// The loop keeps the free memory pointer load from being forwarded to its stored value.
object "MaskedFreePointerWrap" {
    code {
        datacopy(0x80, dataoffset("MaskedFreePointerWrap_deployed"), datasize("MaskedFreePointerWrap_deployed"))
        return(0x80, datasize("MaskedFreePointerWrap_deployed"))
    }
    object "MaskedFreePointerWrap_deployed" {
        code {
            mstore(0x40, add(0x80, and(calldataload(0x20), 0x20)))
            for { let i := 0 } lt(i, iszero(calldatasize())) { i := add(i, 1) } {
                mstore(0x80, i)
            }
            calldatacopy(and(add(mload(0x40), 0xffffffffffffffc0), 0xffffffffffffffff), 0, 0x20)
            mstore(0x80, mload(0x40))
            return(0x80, 0x20)
        }
    }
}
