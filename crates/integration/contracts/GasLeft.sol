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
                        "contract": "GasLeft"
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

contract GasLeft {
    constructor() payable {
        assert(gasleft() > gasleft());
        assert(gasleft() > 0 && gasleft() < 0xffffffffffffffff);
    }
}
