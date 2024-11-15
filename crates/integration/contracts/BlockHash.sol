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

contract BlockHash {
    constructor(bytes32 expected) payable {
        assert(blockhash(0) == expected);
        assert(blockhash(1) == 0);
        assert(
            blockhash(
                0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
            ) == 0
        );
    }
}
