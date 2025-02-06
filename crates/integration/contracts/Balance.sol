// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Upload": {
                "code": {
                    "Solidity": {
                        "contract": "BalanceReceiver"
                    }
                }
            }
        },
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "Balance"
                    }
                },
                "value": 24
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
                "data": "6ada15d90000000000000000000000000303030303030303030303030303030303030303000000000000000000000000000000000000000000000000000000000000000a"
            }
        }
    ]
}
*/

contract BalanceReceiver {
    constructor() payable {}

    fallback() external payable {}
}

contract Balance {
    constructor() payable {
        // 0 to EOA
        transfer_to(payable(address(0xdeadbeef)), 0);
        send_to(payable(address(0xdeadbeef)), 0);

        // 1 to EOA
        transfer_to(payable(address(0xcafebabe)), 1);
        send_to(payable(address(0xcafebabe)), 1);

        BalanceReceiver balanceReceiver = new BalanceReceiver();

        // 0 to contract
        transfer_to(payable(address(balanceReceiver)), 0);
        send_to(payable(address(balanceReceiver)), 0);

        // 1 to contract
        transfer_to(payable(address(balanceReceiver)), 1);
        send_to(payable(address(balanceReceiver)), 1);
    }

    function transfer_to(address payable _dest, uint _amount) public payable {
        _dest.transfer(_amount);
    }

    function send_to(address payable _dest, uint _amount) public payable {
        require(_dest.send(_amount));
    }
}
