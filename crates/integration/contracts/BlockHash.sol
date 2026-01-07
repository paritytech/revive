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
                        "contract": "BlockHash"
                    }
                },
                "data": "e8ec043305d4cfbb51936ae25b50e0a4352d8eaab03d0f66d8d543e65a9a9668"
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
