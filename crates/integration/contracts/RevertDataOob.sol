// SPDX-License-Identifier: MIT

pragma solidity >=0.8.0;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "RevertDataOob"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "eb8ac921000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000ffffffff"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "eb8ac92100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000"
            }
        }
    ]
}
*/

contract RevertDataOob {
    function test(uint256 offset, uint256 len) external {
        assembly {
            mstore(0, 0xcc572cf9)
            mstore(32, offset)
            mstore(64, len)
            let success := call(mul(div(gas(), 64), 63), address(), 0, 28, 68, 0, 0)
            mstore(0, 0xdeadbeef)
            return(0, 32)
        }
    }

    function main(uint256 offset, uint256 len) external pure {
        assembly {
            mstore(0x40, 0)
            revert(offset, len)
        }
    }
}
