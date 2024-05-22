// SPDX-License-Identifier: MIT

pragma solidity ^0.8.24;

contract CreateA {
    address creator;

    constructor() payable {
        creator = msg.sender;
    }
}

contract CreateB {
    receive() external payable {
        new CreateA{value: msg.value}();
    }

    fallback() external {
        new CreateA{salt: hex"01"}();
    }
}
