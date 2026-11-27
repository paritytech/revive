// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

library Increment {
    function increment(uint256 value) external pure returns (uint256) {
        return value + 1;
    }
}
