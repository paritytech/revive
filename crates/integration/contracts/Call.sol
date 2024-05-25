// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract Call {
    function value_transfer(address payable destination) public payable {
        destination.transfer(msg.value);
    }
}
