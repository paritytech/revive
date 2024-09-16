// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Upload": {
                "code": {
                    "Solidity": {
                        "contract": "CreateA"
                    }
                }
            }
        },
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "CreateB"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "value": 10000
            }
        }
    ]
}
*/

contract CreateA {
    constructor() payable {}
}

contract CreateB {
    receive() external payable {
        new CreateA{value: msg.value}();
    }

    fallback() external {
        new CreateA{salt: hex"01"}();
    }
}
