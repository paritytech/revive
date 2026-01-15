// SPDX-License-Identifier: MIT

pragma solidity >=0.6.2;

import "contracts/Library.sol";

/* runner.json
{
    "differential": false,
    "actions": [
        {
            "Instantiate": {
                "origin": "Bob",
                "code": {
                    "Solidity": {
                        "contract": "L",
                        "path": "contracts/Library.sol"
                    }
                }
            }
        },
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "Linked",
                        "path": "contracts/Linked.sol",
                        "libraries": {
                            "contracts/Library.sol": {
                                "L": "0x17bb6d1a8161a52422f86e4460600bdbefc1becd"
                            }
                        }
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 1
                },
                "data": "dffeadd0"
            }
        },
        {
            "VerifyCall": {
                "success": true,
                "output": "000000000000000000000000000000000000000000000000000000000000000a"
            }
        }
    ]
}
*/

contract Linked {
    function main() public returns (uint) {
        return L.f();
    }
}
