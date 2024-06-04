// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract Bitwise {
    function opByte(uint i, uint x) public payable returns (uint ret) {
        assembly {
            ret := byte(i, x)
        }
    }
}
