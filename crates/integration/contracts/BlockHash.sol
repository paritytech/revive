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
                        "contract": "Context"
                    }
                },
                "data": "4545454545454545454545454545454545454545454545454545454545454545"
            }
        }
    ]
}
*/

contract Context {
    constructor(bytes32 expected) payable {
        assert(expected == blockhash(0));
    }
}
