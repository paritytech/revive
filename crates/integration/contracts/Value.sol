// SPDX-License-Identifier: MIT

pragma solidity ^0.8.24;

contract Value {
    function value() public payable returns (uint ret) {
        ret = msg.value;
    }
}
