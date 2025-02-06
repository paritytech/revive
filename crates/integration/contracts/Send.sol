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
                        "contract": "Send"
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
            "VerifyCall": {
                "success": true
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
            "VerifyCall": {
                "success": false
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "fb9e8d050000000000000000000000000000000000000000000000000000000000000000"
            }
        },
        {
            "VerifyCall": {
                "success": false
            }
        }
    ]
}
*/

contract Send {
    constructor() payable {}

    function transfer_self(uint _amount) public payable {
        transfer_to(payable(address(this)), _amount);
    }

    function transfer_to(address payable _dest, uint _amount) public payable {
        if (_dest.send(_amount)) {}
    }

    fallback() external {}

    receive() external payable {}
}
