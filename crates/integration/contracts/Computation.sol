// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/* runner.json
{
    "actions": [
    {
      "Instantiate": {}
    },
    {
      "Call": {
        "dest": {
            "Instantiated": 0
        },
        "data": "0f760610000000000000000000000000000000000000000000000000000000000000000d"
      }
    },
    {
        "VerifyCall": {
            "success": true,
            "output": "000000000000000000000000000000000000000000000000000000000000005b"
        }
    },
    {
      "Call": {
        "dest": {
            "Instantiated": 0
        },
        "data": "00261b660000000000000000000000000000000000000000000000000000000000000005"
      }
    },
    {
        "VerifyCall": {
            "success": true,
            "output": "00000000000000000000000000000000000000000000000000000000000003b1"
        }
    }
  ]
}
*/

contract Computation {
    function triangle_number(int64 n) public pure returns (int64 sum) {
        unchecked {
            for (int64 x = 1; x <= n; x++) {
                sum += x;
            }
        }
    }

    function odd_product(int32 n) public pure returns (int64) {
        unchecked {
            int64 prod = 1;
            for (int32 x = 1; x <= n; x++) {
                prod *= 2 * int64(x) - 1;
            }
            return prod;
        }
    }
}
