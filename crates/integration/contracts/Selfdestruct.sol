// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

// TODO: This currently fails the differential test.
// The pallet doesn't send the correct balance back.

/* runner.json
{
    "differential": false,
    "actions": [
        {
            "Upload": {
                "code": {
                    "Solidity": {
                        "contract": "SelfdestructTester"
                    }
                }
            }
        },
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "Selfdestruct"
                    }
                },
                "value": 123456789
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                }
            }
        }
    ]
}
*/

contract Selfdestruct {
    address tester;
    uint value;

    constructor() payable {
        require(msg.value > 0, "the test should have value");
        value = msg.value;

        SelfdestructTester s = new SelfdestructTester{value: msg.value}();
        tester = address(s);
    }

    fallback() external {
        (bool success, ) = tester.call(hex"");
        require(success, "the call to the self destructing contract should succeed");
    }
}

contract SelfdestructTester {
    constructor() payable {}

    fallback() external {
        selfdestruct(payable(msg.sender));
    }
}
