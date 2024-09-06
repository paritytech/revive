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
        }
    ]
}
*/

contract Value {
    function value() public payable returns (uint ret) {
        ret = msg.value;
    }

    function balance_of(address _address) public view returns (uint ret) {
        ret = _address.balance;
    }
}
