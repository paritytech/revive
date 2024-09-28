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
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "value": 123,
                "data": "1eb16e5b000000000000000000000000d8b934580fce35a11b58c6d73adee468a2833fa8"
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
}

contract Caller {
    function value_transfer(address payable destination) public payable {
        destination.transfer(msg.value);
    }

    function call(bytes memory payload) public returns (bytes memory) {
        Callee callee = new Callee();
        return callee.echo(payload);
    }
}
