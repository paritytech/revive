// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract MStore8 {
    function mStore8(uint value) public pure returns (uint256 word) {
        assembly {
            mstore8(0x80, value)
            word := mload(0x80)
        }
    }
}
