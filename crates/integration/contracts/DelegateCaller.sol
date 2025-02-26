// SPDX-License-Identifier: MIT

pragma solidity ^0.8.28;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "DelegateCaller"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "e466c6c9"
            }
        }
    ]
}
*/

contract DelegateCaller {
    function delegateNoContract() external returns (bool) {
        address testAddress = 0x0000000000000000000000000000000000000000;
        (bool success, ) = testAddress.delegatecall(
            abi.encodeWithSignature("test()")
        );
        return success;
    }
}
