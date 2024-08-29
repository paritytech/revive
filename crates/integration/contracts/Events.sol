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
                        "contract": "Events"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "4d43bec90000000000000000000000000000000000000000000000000000000000000000"
            }
        }
    ]
}
*/

/* TODO when pallet_revive accepts Solidity event topics
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "4d43bec9000000000000000000000000000000000000000000000000000000000000007b"
            }
        }

*/

contract Events {
    event A() anonymous;
    event E(uint, uint indexed, uint indexed, uint indexed);

    function emitEvent(uint topics) public {
        if (topics == 0) {
            emit A();
        } else {
            emit E(topics, 1, 2, 3);
        }
    }
}
