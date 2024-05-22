// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

contract Flipper {
    bool coin;

    constructor(bool _coin) {
        coin = _coin;
    }

    function flip() public {
        coin = !coin;
    }
}
