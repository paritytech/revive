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

/// Exercises the stdlib `__ulongrem` Knuth-D helper via mulmod's slow path
/// (modulus >= 2^128). Inputs come over calldata to defeat solc folding.
contract UlongRem {
    function bigMulMod(uint256 a, uint256 b, uint256 m) external pure returns (uint256) {
        return mulmod(a, b, m);
    }
}

contract UlongRemTester {
    constructor() {
        UlongRem c = new UlongRem();

        // PoC from the bug report. Drives the second-iteration trial-quotient
        // candidate to exactly 2^128 - 1, the boundary the off-by-one loop bound
        // in `__ulongrem` mishandled. Failure mode (pre-fix): PVM returns
        // `correct + m`.
        uint256 a = 1 << 159;
        uint256 b = type(uint256).max - (1 << 54);
        uint256 m = (1 << 255) + 1;
        uint256 r = c.bigMulMod(a, b, m);
        assert(r < m);
    }
}
