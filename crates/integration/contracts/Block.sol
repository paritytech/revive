// SPDX-License-Identifier: MIT

pragma solidity ^0.8.24;

contract Block {
    function timestamp() public view returns (uint ret) {
        ret = block.timestamp;
    }

    function number() public view returns (uint ret) {
        ret = block.number;
    }
}
