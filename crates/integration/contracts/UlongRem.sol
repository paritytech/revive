// SPDX-License-Identifier: GPL-3.0

pragma solidity ^0.8;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Upload": {
                "code": {
                    "Solidity": {
                        "contract": "UlongRem"
                    }
                }
            }
        },
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "UlongRemTester"
                    }
                }
            }
        },
        {
            "VerifyCall": {
                "success": true
            }
        }
    ]
}
*/

/// Exercises the slow path of `mulmod`/`addmod` (modulus >= 2^128), which is
/// lowered through the stdlib `__ulongrem` Knuth-D long-division helper.
/// Inputs come over calldata to defeat Solidity's compile-time constant folding.
contract UlongRem {
    function bigMulMod(uint256 a, uint256 b, uint256 m) external pure returns (uint256) {
        return mulmod(a, b, m);
    }
}

contract UlongRemTester {
    constructor() {
        UlongRem c = new UlongRem();

        // PoC operands from the bug report: drive __ulongrem's trial-quotient
        // candidate into [divisor, 2*divisor), so the missing Knuth-D final
        // correction step returns `correct + m` instead of `correct`.
        uint256 a = 1 << 159;
        uint256 b = type(uint256).max - (1 << 54); // 2^256 - 2^54 - 1
        uint256 m = (1 << 255) + 1;

        uint256 result = c.bigMulMod(a, b, m);

        // Fundamental postcondition of mulmod: result < modulus.
        assert(result < m);
    }
}
