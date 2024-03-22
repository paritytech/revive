// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract Flipper {
    bool coin;

    function flip() public payable {
        coin = !coin;
    }
}
