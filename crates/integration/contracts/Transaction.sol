// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Upload": {
                "code": {
                    "Solidity": {
                        "contract": "TransactionOrigin"
                    }
                }
            }
        },
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "TransactionTester"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "f8a8fd6d"
            }
        }
    ]
}
*/

contract TransactionTester {
    constructor() payable {
        assert(tx.origin == new TransactionOrigin().test());
    }

    function test() public payable returns (address ret) {
        ret = tx.origin;
    }
}

contract TransactionOrigin {
    function test() public payable returns (address ret) {
        assert(msg.sender != tx.origin);

        ret = tx.origin;
    }
}
