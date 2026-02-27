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
                        "contract": "GasLimit"
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

contract GasLimit {
    constructor() payable {
        assert(block.gaslimit > 0);
    }
}
