// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

// Regression test: factorial with uint8 parameter and uint64 result.
// Bug: newyork pipeline doesn't revert for out-of-range uint8 parameter,
// causing the function to return 1 (factorial of truncated 0) instead of
// reverting.

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": { "Solidity": { "contract": "Factorial", "solc_optimizer": false } }
            }
        },
        {
            "Call": {
                "dest": { "Instantiated": 0 },
                "data": "ebbee3910000000000000000000000000000000000000000000000000000000000000005"
            }
        },
        {
            "Call": {
                "dest": { "Instantiated": 0 },
                "data": "ebbee3910000000000000000000000000000000000000000000000000000000100000000"
            }
        }
    ]
}
*/

contract Factorial {
    function main(uint8 n) public pure returns (uint64) {
        uint64 fact = 1;
        for (uint8 i = 1; i <= n; i++) {
            fact *= i;
        }
        return fact;
    }
}
