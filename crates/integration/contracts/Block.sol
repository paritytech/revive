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
                        "contract": "Block"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "8381f58a"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "b80777ea"
            }
        }
    ]
}
*/

contract Block {
    function timestamp() public view returns (uint ret) {
        ret = block.timestamp;
    }

    function number() public view returns (uint ret) {
        if (block.number == 0) {
            ret = 1;
        } else {
            ret = block.number;
        }
    }
}
