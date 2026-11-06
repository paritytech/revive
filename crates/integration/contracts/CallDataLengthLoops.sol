// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract CallDataLengthLoops {
    fallback(bytes calldata) external returns (bytes memory) {
        uint256 sum;
        for (uint256 index = 0; index < msg.data.length; index++) {
            sum += index;
        }
        for (uint256 index = 0; index < msg.data.length; index++) {
            sum += index;
        }
        return abi.encode(sum);
    }
}
