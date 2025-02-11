// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "Coinbase"
                    }
                }
            }
        }
    ]
}
*/

contract Coinbase {
    constructor() payable {
        address coinbase = address(0xFFfFfFffFFfffFFfFFfFFFFFffFFFffffFfFFFfF);
        assert(block.coinbase == coinbase);
    }
}
