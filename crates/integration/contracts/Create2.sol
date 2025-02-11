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
                },
                "value": 100000
            }
        }
    ]
}
*/

contract CreateA {
    constructor() payable {}
}

contract CreateB {
    constructor() payable {
        bytes32 salt = hex"ff";

        try new CreateA{salt: salt}() returns (CreateA) {} catch {
            revert("the first instantiation should succeed");
        }

        try new CreateA{salt: salt}() returns (CreateA) {} catch {
            return;
        }

        revert("the second instantiation should have failed");
    }
}
