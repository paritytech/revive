// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract CustomErrorRevertMemoryLoad {
    fallback() external {
        assembly {
            mstore(0, shl(224, 0xdeadbeef))
            mstore(4, mload(calldataload(0)))
            revert(0, 0x24)
        }
    }
}
