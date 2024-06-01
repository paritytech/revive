// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract Call {
    function value_transfer(address payable destination) public payable {
        destination.transfer(msg.value);
    }

    function echo(bytes memory payload) public pure returns (bytes memory) {
        return payload;
    }

    function call(
        address callee,
        bytes memory payload
    ) public pure returns (bytes memory) {
        return Call(callee).echo(payload);
    }
}
