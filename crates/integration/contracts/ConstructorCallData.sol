// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract ConstructorCallData {
    uint256 public n;
    constructor(uint256 x) {
        n = msg.data.length + x;
    }
}
