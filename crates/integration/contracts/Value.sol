// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "value": 1024,
                "code": {
                    "Solidity": {
                        "contract": "Value"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "value": 123,
                "data": "3fa4f245"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "52da5fa0"
            }
        }
    ]
}
*/

contract Value {
    constructor() payable {}

    function value() public payable returns (uint ret) {
        ret = msg.value;
    }

    function balance_self() public view returns (uint ret) {
        ret = address(this).balance;
    }

    function balance_of(address _address) public view returns (uint ret) {
        ret = _address.balance;
    }
}
