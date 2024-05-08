// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract Context {
    function address_this() public view returns (address ret) {
        ret = address(this);
    }

    function caller() public view returns (address ret) {
        ret = msg.sender;
    }
}
