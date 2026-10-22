// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract CalldataCopyOverFreePointer {
    fallback() external {
        assembly {
            calldatacopy(0, 0, calldatasize())
            sstore(0, mload(0x30))
            if iszero(gt(mload(0x40), 0xffffffff)) { revert(0, 0) }
        }
    }
}
