// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

// Regression test: sub-underflow result widened to uint256 must preserve EVM
// modular semantics.
//
// Bug (crates/newyork/src/to_llvm.rs ~5977-5999): when `BinaryOperation::Sub`
// is codegen'd with both operands LLVM-narrowed to <= 64 bits (via
// `try_narrow_let_binding` Strategy 1 — structural proof from `and value, mask`)
// and the consumer's demand for the let binding holding the sub result is None
// (here: the result is mstored as a uint256), codegen takes the i128 fast-path:
//   sub at i128 of (0, 1) → 2^128 - 1 (high bit set)
// Later widening via ensure_word_type → build_int_z_extend produces
// 0x...0000FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF instead of 2^256 - 1.
//
// Inline assembly is used so the masks land as IR constants (Solidity's
// non-asm path wraps integer literals through a `convert_rational_*` function,
// which defeats the structural narrowness proof).

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": { "Solidity": { "contract": "SubUnderflowZext", "solc_optimizer": false } }
            }
        },
        {
            "Call": {
                "dest": { "Instantiated": 0 },
                "data": "0aa7a67700000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001"
            }
        }
    ]
}
*/

contract SubUnderflowZext {
    function sub_underflow(uint256 a, uint256 b) external pure returns (uint256 r) {
        assembly {
            let x := and(a, 0xff)
            let y := and(b, 0xff)
            r := sub(x, y)
        }
    }
}
