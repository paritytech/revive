// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract BlobOpcodes {
    uint256 public valueAtDeploy;

    constructor() {
        valueAtDeploy = block.blobbasefee + uint256(blobhash(0));
    }

    function blobBaseFee() external view returns (uint256) {
        return block.blobbasefee;
    }

    function blobHash(uint256 index) external view returns (bytes32) {
        return blobhash(index);
    }
}
