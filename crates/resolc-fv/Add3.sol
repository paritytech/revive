// SPDX-License-Identifier: MIT

pragma solidity >=0.7.0 <0.9.0;

contract Add3 {
    /// Sums three signed integers through an explicit intermediate, so the
    /// addition shows up as two separate operations in the IR.
    function add3(int256 a, int256 b, int256 c) public pure returns (int256 res) {
        int256 tmp = a + b;
        res = tmp + c;
    }
}
