// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Soundness PoC: `to_llvm.rs::get_or_create_return_block` (and the
/// emit-revert sibling) call `context.xlen_type().const_int(const_offset,
/// false)` to build the constant offset/length for the
/// `emit_exit_unchecked` path. `const_int` takes a `u64` and truncates
/// silently to `xlen_type`'s width (`i32`), so any constant
/// offset/length above `u32::MAX` (but still within `u64::MAX` so
/// `try_extract_const_u64` returns `Some`) is wrapped mod `2^32` and the
/// resulting return/revert reads from / aliases the truncated heap
/// offset. EVM expands memory and OOGs.
contract ConstReturnOverflowBug {
    function bug() external pure returns (uint256 r) {
        assembly {
            // Constant offset 2^56 — fits in u64, doesn't fit in u32. With
            // the current codegen, the shared `return_shared_…` block
            // truncates to i32 (=0) and reads/writes at heap[0..32]
            // instead of trapping.
            mstore(0x100000000000000, mload(0x100000000000000))
            return(0x100000000000000, 32)
            // `r` is just to satisfy the signature; control flow exits
            // via the inline-assembly `return` above.
            r := 0
        }
    }
}
