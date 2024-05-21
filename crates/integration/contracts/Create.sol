// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract CreateA {
    address creator;

    constructor() {
        creator = msg.sender;
    }
}

contract CreateB {
    receive() external payable {}

    fallback() external payable {
        new CreateA();
    }
}
