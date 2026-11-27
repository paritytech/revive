// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

library AddOne {
    function add(uint256 value) external pure returns (uint256) {
        return value + 1;
    }
}

library AddTwo {
    function add(uint256 value) external pure returns (uint256) {
        return value + 2;
    }
}

contract TwoLibraries {
    function sum(uint256 value) external pure returns (uint256) {
        return AddOne.add(value) + AddTwo.add(value);
    }
}
