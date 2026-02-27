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
                        "contract": "Transfer"
                    }
                },
                "value": 211
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "1c8d16b30000000000000000000000000303030303030303030303030303030303030303000000000000000000000000000000000000000000000000000000000000000a"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "fb9e8d050000000000000000000000000000000000000000000000000000000000000001"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "fb9e8d050000000000000000000000000000000000000000000000000000000000000000"
            }
        }
    ]
}
*/

contract Transfer {
    constructor() payable {}

    function transfer_self(uint _amount) public payable {
        transfer_to(payable(address(this)), _amount);
    }

    function transfer_to(address payable _dest, uint _amount) public payable {
        _dest.transfer(_amount);
    }

    fallback() external {}

    receive() external payable {}
}
