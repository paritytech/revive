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
                        "contract": "TestCalldata"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "d45754f800000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001a300000000000000000000000000000000000000000000000000000000000000"
            }
        }
    ]
}
*/

contract TestCalldata {
    function bad_func_dispatch(bytes memory data) external payable returns (uint256) {
        uint256 result;
        assembly {
            let ptr := mload(add(data, 0x20))
            result := calldataload(ptr)
        }
        return result;
    }
}
