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
                        "contract": "Context"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "846a1ee1"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "fc9c8d39"
            }
        }
    ]
}
*/

contract Context {
    function address_this() public view returns (address ret) {
        ret = address(this);
    }

    function caller() public view returns (address ret) {
        ret = msg.sender;
    }
}
