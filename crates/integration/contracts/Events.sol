// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

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
