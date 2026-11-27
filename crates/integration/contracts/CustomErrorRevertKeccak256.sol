// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract CustomErrorRevertKeccak256 {
    fallback() external {
        assembly {
            mstore(0, shl(224, 0xdeadbeef))
            mstore(4, keccak256(calldataload(0), 0x20))
            revert(0, 0x24)
        }
    }
}
