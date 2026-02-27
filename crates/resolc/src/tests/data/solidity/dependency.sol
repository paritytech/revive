// SPDX-License-Identifier: GPL-3.0

pragma solidity >=0.7.0 <0.9.0;

library Assert {
    function equal(uint256 a, uint256 b) public pure returns (bool result) {
    result = (a == b);
  }
}

library AssertNe {
    function notEqual(uint256 a, uint256 b) public pure returns (bool result) {
    result = (a != b);
  }
}

contract TestAssert {
    constructor() payable {
        new Dependency(); 
    }

    function checkEquality(uint256 a, uint256 b) public pure returns (string memory) {
        Assert.equal(a, b);
        return "Values are equal";
    }
}

contract Dependency {
    function checkNotEquality(uint256 a, uint256 b) public pure returns (string memory) {
        AssertNe.notEqual(a, b);
        return "Values are not equal";
    }
}
