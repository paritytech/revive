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
                        "contract": "GasPrice"
                    }
                },
                "data": "4545454545454545454545454545454545454545454545454545454545454545"
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

contract GasPrice {
    constructor(uint expected) payable {
        assert(tx.gasprice == expected);
    }
}
