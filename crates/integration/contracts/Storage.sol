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
                        "contract": "Storage"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "fabc9efaffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "558b9f9bffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
            }
        }
    ]
}
*/

contract Storage {
    function transient(uint value) public returns (uint ret) {
        assembly {
            let slot := 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00
            tstore(slot, value)
            let success := call(0, 0, 0, 0, 0, 0, 0)
            ret := tload(slot)
        }
    }

    function persistent(uint value) public returns (uint ret) {
        assembly {
            let slot := 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00
            sstore(slot, value)
            ret := sload(slot)
        }
    }
}
