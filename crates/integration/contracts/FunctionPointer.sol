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
                        "contract": "FunctionPointer"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "26121ff0"
            }
        }
    ]
}
*/

contract FunctionPointer {
    bool public flag = false;

    function f0() public {
        flag = true;
    }

    function f() public returns (bool) {
        function() internal x = f0;
        x();
        return flag;
    }
}
