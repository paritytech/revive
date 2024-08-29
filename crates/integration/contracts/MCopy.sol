// SPDX-License-Identifier: MIT

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "MCopy"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "0ee188b0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000030102030000000000000000000000000000000000000000000000000000000000"
            }
        }
    ]
}
*/

pragma solidity ^0.8;

contract MCopy {
    function memcpy(bytes memory payload) public pure returns (bytes memory) {
        return payload;
    }
}
