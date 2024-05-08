// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

contract Flipper {
    bool coin;

    function flip() public {
        coin = !coin;
    }
}
