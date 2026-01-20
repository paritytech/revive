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
                        "contract": "StructDeleteStorage"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "e70c830d"
            }
        }
    ]
}
*/

contract StructDeleteStorage {
    struct S {
        uint8 a;
    }
    S s;

    function store_and_delete() public returns (uint256 r1) {
        assembly {
            sstore(s.slot, 0xffffff)
        }
        delete s;
        assembly {
            r1 := sload(s.slot)
        }
    }
}
