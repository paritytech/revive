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
                        "contract": "ValueTester"
                    }
                }
            }
        },
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
        }
    ]
}
*/

contract ValueTester {
    constructor() payable {}

    function balance_self() public view returns (uint ret) {
        ret = address(this).balance;
    }
}

contract Value {
    constructor() payable {
        ValueTester tester = new ValueTester{value: msg.value}();

        // own account
        assert(address(this).balance == 0);

        // tester account
        assert(address(tester).balance == msg.value);
        assert(tester.balance_self() == msg.value);

        // non-existant account
        assert(address(0xdeadbeef).balance == 0);
    }

    function value() public payable returns (uint ret) {
        ret = msg.value;
    }
}
