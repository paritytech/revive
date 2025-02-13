// SPDX-License-Identifier: MIT

pragma solidity ^0.8.28;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "MLoad"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "e2179b8e"
            }
        }
    ]
}
*/

contract MLoad {
    constructor() payable {
        assert(g() == 0);
    }

    function g() public payable returns (uint m) {
        assembly {
            m := mload(0)
        }
    }
}
