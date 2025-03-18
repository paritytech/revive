// SPDX-License-Identifier: MIT

pragma solidity ^0.8.29;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "LayoutAt"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "a7a0d537"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "15393349"
            }
        }
    ]
}
*/

contract LayoutAt layout at 0xDEADBEEF + 0xCAFEBABE {
    uint[3] public something;

    constructor() payable {
        something[0] = 1337;
        something[1] = 42;
        something[2] = 69;
    }

    function slotOfSomething() public pure returns (uint ret) {
        assembly {
            ret := something.slot
        }
    }
}
