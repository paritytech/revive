// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/* runner.json
{
    "differential": false,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "BaseFee"
                    }
                }
            }
        },
        {
            "VerifyCall": {
                "success": true
            }
        }
    ]
}
*/

contract BaseFee {
    constructor() payable {
        assert(block.basefee > 0);
    }
}
