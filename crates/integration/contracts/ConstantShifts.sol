// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Constant shifts of a 32-bit value on both sides of the 64-bit boundary.
// This exercises both the 64-bit `shl` path (operand width plus shift at most 64) and the word fallback.
// The chained shift keeps its pair through an or and is stored unmasked, so its outer shl narrows on the operand width rather than on demanded bits.

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "ConstantShifts"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "b8ef11e200000000000000000000000000000000000000000000000000000000deadbeef"
            }
        }
    ]
}
*/

contract ConstantShifts {
    function shifts(uint32 value) external pure returns (uint256, uint64, uint256, uint256, uint32, uint256) {
        uint256 withinSixtyFour = uint256(value) << 31;
        uint64 exactlySixtyFour = uint64(value) << 32;
        uint256 pastSixtyFour = uint256(value) << 33;
        uint256 farPastSixtyFour = uint256(value) << 200;
        uint32 rotated = (value << 7) | (value >> 25);
        uint256 chained = ((uint256(value) >> 10) | 1) << 20;
        return (withinSixtyFour, exactlySixtyFour, pastSixtyFour, farPastSixtyFour, rotated, chained);
    }
}
