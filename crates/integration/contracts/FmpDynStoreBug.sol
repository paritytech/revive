// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Soundness PoC: `mem_opt::FmpPropagation::propagate_statements`
/// tracks the known FMP value across statements. Its MStore handler
/// detects FMP writes by `region == FreePointerSlot ||
/// resolved_offset == Some(0x40)`. For an mstore with a *dynamic*
/// offset whose runtime value happens to be 0x40 (but the simplifier
/// can't see that — e.g. `calldataload(...)`), neither condition
/// fires: the simplifier leaves `region == Unknown` because the
/// offset isn't a literal, and `resolve_offset` returns None because
/// the value isn't tracked in the local constants map. The
/// `fmp_value` stays stale.
///
/// A subsequent `mload(0x40)` IS recognized as an FMP load (because
/// 0x40 is a literal). FmpPropagation folds it to the stale literal
/// `fmp_value` at IR level — diverging from EVM which reads the
/// overwritten memory.
///
/// Both values (`0x100`, `0x200`) fit in the heap-size range so the
/// FMP-load range proof (`to_llvm.rs::apply_range_proof`) doesn't
/// affect this test.
contract FmpDynStoreBug {
    fallback() external {
        uint256 offset;
        assembly {
            // Set FMP via a recognized FMP-slot mstore.
            mstore(0x40, 0x100)
            // Caller passes 0x40 as calldata so offset = 0x40 at
            // runtime, but the simplifier sees an opaque calldataload.
            offset := calldataload(0)
            // Dynamic-offset mstore. FmpPropagation can't tell this
            // hits 0x40, so it doesn't invalidate the known fmp_value.
            mstore(offset, 0x200)
            let v := mload(0x40)
            mstore(0x100, v)
            return(0x100, 32)
        }
    }
}
