// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

// Regression test: arithmetic with uint128 types.
// Bug: newyork pipeline produces wrong results for uint128 arithmetic with
// values that trigger overflow checking in Solidity 0.8+.

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": { "Solidity": { "contract": "Uint128Arithmetic", "solc_optimizer": false } }
            }
        },
        {
            "Call": {
                "dest": { "Instantiated": 0 },
                "data": "ba517410000000000000000000000000000000000000000000000000000000000000002100000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000100000000000000000000000000000000"
            }
        },
        {
            "Call": {
                "dest": { "Instantiated": 0 },
                "data": "ba51741000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000001f"
            }
        }
    ]
}
*/

contract Uint128Arithmetic {
    uint128 constant EPS = 1E10;
    uint128 constant PRECISION = 100;
    uint128 constant MAX_U128_SQRT = 18446744073709551615;

    function entry(uint128 a, uint128 mb, uint128 c) public pure returns (bool, uint128, uint128) {
        (bool p, uint128 x1, uint128 x2) = main(a, mb, c);
        x1 *= PRECISION;
        x1 /= EPS;
        x2 *= PRECISION;
        x2 /= EPS;
        return (p, x1, x2);
    }

    function sqrt(uint128 n) private pure returns (uint128) {
        uint128 l = 0;
        uint128 r = MAX_U128_SQRT;
        while (l < r) {
            uint128 m = (l + r + 1) / 2;
            if (m * m <= n) {
                l = m;
            } else {
                r = m - 1;
            }
        }
        if (n - l * l < (l + 1) * (l + 1) - n) {
            return l;
        } else {
            return l + 1;
        }
    }

    function main(uint128 a, uint128 mb, uint128 c) private pure returns (bool, uint128, uint128) {
        if (mb * mb < 4 * a * c) {
            return (false, 0, 0);
        }
        uint128 d = (mb * mb - 4 * a * c) * EPS * EPS;
        uint128 sd = sqrt(d);
        uint128 x1 = (mb * EPS + sd) / 2 / a;
        uint128 x2 = (mb * EPS - sd) / 2 / a;
        return (true, x1, x2);
    }
}
