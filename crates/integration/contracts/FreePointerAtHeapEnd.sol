// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract FreePointerAtHeapEnd {
    error E(uint256 p);
    fallback() external {
        new bytes(abi.decode(msg.data, (uint256)));
        uint256 p;
        assembly { p := mload(0x40) }
        revert E(p);
    }
}
