// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

import "dependency/increment.sol";

contract Main {
    function incremented(uint256 value) external pure returns (uint256) {
        return Increment.increment(value);
    }
}
