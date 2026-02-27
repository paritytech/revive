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
                "data": "0be0e4a60000000000000000000000000000000000000000000000000000000000000000"
            }
        }
    ]
}
*/

contract MLoad {
    constructor() payable {
        assert(loadAt(0) == 0);
    }

    function loadAt(uint _offset) public payable returns (uint m) {
        assembly {
            m := mload(_offset)
        }
    }
}
