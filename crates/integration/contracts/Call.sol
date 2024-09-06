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
                        "contract": "Call"
                    }
                }
            }
        },
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "Call"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 1
                },
                "data": "1b8b921d0000000000000000000000001c81a61a407017c58397a47d2ab28191b9b8ec9b000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000050102030405000000000000000000000000000000000000000000000000000000"
            }
        }
    ]
}
*/

contract Call {
    function value_transfer(address payable destination) public payable {
        destination.transfer(msg.value);
    }

    function echo(bytes memory payload) public pure returns (bytes memory) {
        return payload;
    }

    function call(
        address callee,
        bytes memory payload
    ) public pure returns (bytes memory) {
        return Call(callee).echo(payload);
    }
}
