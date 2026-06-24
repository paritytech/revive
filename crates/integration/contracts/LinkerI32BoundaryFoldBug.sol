// SPDX-License-Identifier: MIT

pragma solidity ^0.8.20;

/// Regression: at `-O3` (the `OptimizerSettings::cycles()` middle/back-end
/// level used by the differential harness) LLVM proves the low 64 bits of
/// `(a2 + (a0 << 64)) ^ a2` cancel to zero and narrows the surviving
/// `| 0x80000000 | 0x80000001` work down to native i32 lane operations.
/// The constant `0x80000000` (`2^31`) is materialized with `lui x, 0x80000`.
/// During link time the polkavm-linker (`program_from_elf.rs`) constant-folds
/// the narrowed 32-bit op through `OperationKind::apply_const`, whose `op32!`
/// macro does `i64 -> i32` via `try_into().expect("operand overflow")`. The
/// tracked constant for `0x80000000` does not fit the signed i32 range, so the
/// fold panics with `operand overflow: TryFromIntError` — an ICE instead of a
/// clean compile. solc's EVM backend has no such narrowing (everything is
/// 256-bit), so the same source compiles cleanly there.
contract LinkerI32BoundaryFoldBug {
    function test(int256 a0, int256 a2) external pure returns (int256) {
        int256 t1 = (a2 + (a0 * int256(18446744073709551616))) ^ a2; // low-64 cancel
        t1 = t1 | int256(2147483648); // 0x80000000  (i32 sign boundary)
        t1 = t1 | int256(2147483649); // 0x80000001
        return t1;
    }
}
