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
                        "contract": "MCopyOverlap"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "afdce848"
            }
        }
    ]
}
*/

function copy(
    uint dstOffset,
    uint srcOffset,
    uint length
) pure returns (bytes memory out) {
    out = hex"2222222222222222333333333333333344444444444444445555555555555555"
    hex"6666666666666666777777777777777788888888888888889999999999999999"
    hex"aaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbccccccccccccccccdddddddddddddddd";
    assembly {
        mcopy(
            add(add(out, 0x20), dstOffset),
            add(add(out, 0x20), srcOffset),
            length
        )
    }
}

contract MCopyOverlap {
    function mcopy_to_right_overlap() public pure returns (bytes memory) {
        return copy(0x20, 0x10, 0x30);
    }
}
