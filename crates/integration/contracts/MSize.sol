// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/* runner.json
{
    "actions": [
    {
        "Instantiate": {
            "code": {
                "Solidity": {
                    "contract": "MSize",
                    "solc_optimizer": false
                }
            }
        }
    },
    {
        "Call": {
            "dest": {
                "Instantiated": 0
            },
            "data": "f016832c"
        }
    },
    {
        "VerifyCall": {
            "success": true,
            "output": "0000000000000000000000000000000000000000000000000000000000000060"
        }
    },
    {
        "Call": {
            "dest": {
                "Instantiated": 0
            },
            "data": "f4a63aa5"
        }
    },
    {
        "VerifyCall": {
            "success": true,
            "output": "0000000000000000000000000000000000000000000000000000000000000084"
        }
    }
  ]
}
*/

contract MSize {
    uint[] public data;

    function mSize() public pure returns (uint size) {
        assembly {
            size := msize()
        }
    }

    function mStore100() public pure returns (uint size) {
        assembly {
            mstore(100, msize())
            size := msize()
        }
    }
}
