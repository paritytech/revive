// SPDX-License-Identifier: MIT

pragma solidity ^0.8.28;

contract TransientCounter {
    uint256 transient counter;

    function value() external pure returns (uint256) {
        return 1;
    }
}
