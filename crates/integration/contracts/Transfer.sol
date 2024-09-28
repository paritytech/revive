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
                        "contract": "Transfer"
                    }
                },
                "value": 11
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
                "data": "fb9e8d0500000000000000000000000003030303030303030303030303030303030303030000000000000000000000000000000000000000000000000000000000000001"
            }
        }
    ]
}
*/

contract Transfer {
    constructor() payable {
        transfer_self(msg.value);
    }

    function address_self() internal view returns (address payable) {
        return payable(address(this));
    }

    function transfer_self(uint _amount) public payable {
        transfer_to(address_self(), _amount);
    }

    function transfer_to(address payable _dest, uint _amount) public payable {
        _dest.transfer(_amount);
    }
}
