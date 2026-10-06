// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract BL {
    uint256 public atDeploy;
    constructor() {
        atDeploy = block.blobbasefee + uint256(blobhash(0));
    }
    function fee() external view returns (uint256) {
        return block.blobbasefee;
    }
    function hash(uint256 i) external view returns (bytes32) {
        return blobhash(i);
    }
}
