// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/* runner.json
{
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "ExtCode"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "cbb20918"
            }
        },
        {
            "VerifyCall": {
                "success": true,
                "output": "10b3daf252aa843eae2b3d850da130d6a41ce1c23f33081c7fd8e48a0844add8"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "88d6a2330000000000000000000000001c81a61a407017c58397a47d2ab28191b9b8ec9b"
            }
        },
        {
            "VerifyCall": {
                "success": true,
                "output": "10b3daf252aa843eae2b3d850da130d6a41ce1c23f33081c7fd8e48a0844add8"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "88d6a233000000000000000000000000ff000000000000000000000000000000000000ff"
            }
        },
        {
            "VerifyCall": {
                "success": true,
                "output": "0000000000000000000000000000000000000000000000000000000000000000"
            }
        }
    ]
}
*/

contract ExtCode {
    function ExtCodeSize(address who) public view returns (uint ret) {
        assembly {
            ret := extcodesize(who)
        }
    }

    function CodeSize() public pure returns (uint ret) {
        assembly {
            ret := codesize()
        }
    }

    function ExtCodeHash(address who) public view returns (bytes32 ret) {
        assembly {
            ret := extcodehash(who)
        }
    }

    function ExtCodeHash() public view returns (bytes32 ret) {
        assembly {
            ret := extcodehash(address())
        }
    }
}
