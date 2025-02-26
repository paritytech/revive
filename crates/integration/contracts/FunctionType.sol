// SPDX-License-Identifier: MIT

pragma solidity ^0.8.28;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "FunctionType"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "b8c9d365"
            }
        }
    ]
}
*/

contract FunctionType {
    uint public immutable x = 42;

    function h() public view returns (function() external view returns (uint)) {
        return this.x;
    }
}
