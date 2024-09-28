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
                        "contract": "ReturnDataOob"
                    }
                }
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

contract Callee {
    function echo(bytes memory payload) public pure returns (bytes memory) {
        return payload;
    }
}

contract ReturnDataOob {
    fallback() external {
        new Callee().echo(hex"1234");
        assembly {
            let pos := mload(64)
            let size := add(returndatasize(), 1)
            returndatacopy(pos, 0, size)
        }
    }
}
