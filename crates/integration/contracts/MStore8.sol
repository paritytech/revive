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
                        "contract": "MStore8"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "b09ad4210000000000000000000000000000000000000000000000000000000000000000"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "b09ad4210000000000000000000000000000000000000000000000000000000000000001"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "b09ad4210000000000000000000000000000000000000000000000000000000000000002"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "b09ad42100000000000000000000000000000000000000000000000000000000000000ff"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "b09ad4210000000000000000000000000000000000000000000000000000000000000100"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "b09ad4210000000000000000000000000000000000000000000000000000000000000101"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "b09ad4210000000000000000000000000000000000000000000000000000000000000102"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "b09ad42100000000000000000000000000000000000000000000000000000000075bcd15"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "b09ad421ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
            }
        }
    ]
}
*/

contract MStore8 {
    function mStore8(uint value) public pure returns (uint256 word) {
        assembly {
            mstore8(0x80, value)
            word := mload(0x80)
        }
    }
}
