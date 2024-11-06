// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "Bitwise"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "3fa4f245"
            }
        }
    ]
}
*/

contract Bitwise {
    function opByte(uint i, uint x) public payable returns (uint ret) {
        assembly {
            ret := byte(i, x)
        }
    }
}
