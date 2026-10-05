// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

contract C {
    function h(uint256 x) external pure returns (uint256) {
        require(x > 1, "too small");
        return x;
    }
}
