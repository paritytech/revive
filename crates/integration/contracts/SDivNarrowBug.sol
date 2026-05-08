// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract SDivNarrowBug {
    /// Forces LLVM IR of the form
    ///   sdiv i256 (and i256 %a, 0xff..ff_u64), (and i256 %b, 0xff..ff_u64)
    ///
    /// `provable_bit_width` accepts the AND-mask proof and lets revive's
    /// `narrow_divrem_instructions` rewrite this to `sdiv i64 (trunc..), (trunc..)`
    /// followed by `sext` back to i256. The truncated low-64 view of an i256
    /// whose bit 63 is set looks negative under i64, but the i256 value is
    /// non-negative (high 192 bits are zero by the AND), so the narrowed
    /// signed division returns the wrong sign.
    function sdiv_masked(uint256 a, uint256 b) external pure returns (int256 q) {
        assembly {
            let am := and(a, 0xffffffffffffffff)
            let bm := and(b, 0xffffffffffffffff)
            q := sdiv(am, bm)
        }
    }
}
