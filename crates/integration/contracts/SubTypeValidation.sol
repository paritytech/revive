// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

// Regression test: ABI parameter validation for sub-256-bit types.
// Solidity 0.8+ reverts when a calldataload value exceeds the range of its
// declared type (e.g., passing 256 to a uint8 parameter).
// Bug: the newyork pipeline silently truncates instead of reverting.

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": { "Solidity": { "contract": "SubTypeValidation", "solc_optimizer": false } }
            }
        },
        {
            "Call": {
                "dest": { "Instantiated": 0 },
                "data": "d552145b0000000000000000000000000000000000000000000000000000000100000000"
            }
        },
        {
            "Call": {
                "dest": { "Instantiated": 0 },
                "data": "7440c86b0000000000000000000000000000000000000000000000010000000000000000"
            }
        },
        {
            "Call": {
                "dest": { "Instantiated": 0 },
                "data": "0850cee20000000000000000000000000000000100000000000000000000000000000000"
            }
        }
    ]
}
*/

contract SubTypeValidation {
    // Should revert when called with value > 0xFF
    function narrow_uint8(uint8 x) public pure returns (uint8) {
        return x;
    }

    // Should revert when called with value > 0xFFFFFFFFFFFFFFFF
    function narrow_uint64(uint64 x) public pure returns (uint64) {
        return x;
    }

    // Should revert when called with value > 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF
    function narrow_uint128(uint128 x) public pure returns (uint128) {
        return x;
    }
}
