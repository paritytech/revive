// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Soundness PoC: same shape as `FmpRevertBug` but with *dynamic*
/// offset and length on the revert. The original `mark_escaping_range`
/// only sets `has_dynamic_escapes` for fully-dynamic escapes
/// (`(None, _)` arm) — it does not lower `min_dynamic_escape_start`,
/// so `fmp_native_safe()` doesn't reject and `mstore(0x40, …)` still
/// encodes as a 4-byte i32 LE native store. With the caller passing
/// `offset = 0` and `length = 96`, the revert covers the FMP slot
/// and the BE/LE encoding mismatch is observed.
contract FmpDynRevertBug {
    fallback() external {
        uint256 offset;
        uint256 length;
        assembly {
            mstore(0x40, 0xdeadbeef)
            offset := calldataload(0)
            length := calldataload(32)
            revert(offset, length)
        }
    }
}
