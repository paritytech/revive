// SPDX-License-Identifier: MIT

pragma solidity ^0.8.28;

contract OptimizerRuns {
    uint256 total;

    function addAndTriple(uint256 amount) external returns (uint256) {
        total += amount;
        return total * 3;
    }
}
