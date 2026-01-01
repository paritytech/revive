// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "value": 0,
                "code": {
                    "Solidity": {
                        "contract": "CalldataTester"
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

contract Calldata {
    function bad_func_dispatch(bytes memory data) public pure returns (uint256, bytes memory) {
        uint256 result1;
        assembly {
            let ptr := mload(add(data, 0x20))
            result1 := calldataload(ptr)
        }
        return (result1, data);
    }
}
