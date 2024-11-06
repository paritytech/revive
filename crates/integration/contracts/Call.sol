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
                        "contract": "Callee"
                    }
                }
            }
        },
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "Caller"
                    }
                },
                "value": 123
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "5a6535fc00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000004cafebabe00000000000000000000000000000000000000000000000000000000"
            }
        }
    ]
}
*/

contract Callee {
    function echo(bytes memory payload) public pure returns (bytes memory) {
        return payload;
    }

    receive() external payable {}
}

contract Caller {
    constructor() payable {
        Callee callee = new Callee();
        payable(address(callee)).transfer(msg.value);
    }

    function call(bytes memory payload) public returns (bytes memory) {
        Callee callee = new Callee();
        return callee.echo(payload);
    }
}
