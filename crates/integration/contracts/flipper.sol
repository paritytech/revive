// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

/* runner.json
{
  "actions": [
    {
      "Instantiate": {
        "data": "0000000000000000000000000000000000000000000000000000000000000001"
      }
    },
    {
        "VerifyStorage": {
            "contract": {
                "Instantiated": 0
            },
            "key": "0000000000000000000000000000000000000000000000000000000000000000",
            "expected": "0000000000000000000000000000000000000000000000000000000000000001"
        }
    },
    {
      "Call": {
        "dest": {
            "Instantiated": 0
        },
        "data": "cde4efa9"
      }
    },
    {
        "VerifyStorage": {
            "contract": {
                "Instantiated": 0
            },
            "key": "0000000000000000000000000000000000000000000000000000000000000000",
            "expected": "0000000000000000000000000000000000000000000000000000000000000000"
        }
    }
  ]
}
*/

contract Flipper {
    bool coin;

    constructor(bool _coin) {
        coin = _coin;
    }

    function flip() public {
        coin = !coin;
    }
}
