// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract Value {
    function value() public payable returns (uint ret) {
        ret = msg.value;
    }

    function balance_of(address _address) public view returns (uint ret) {
        ret = _address.balance;
    }
}
