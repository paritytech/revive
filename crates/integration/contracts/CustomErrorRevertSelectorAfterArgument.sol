// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract CustomErrorRevertSelectorAfterArgument {
    fallback() external {
        assembly {
            mstore(4, not(0))
            mstore(0, shl(224, 0xdeadbeef))
            revert(0, 0x24)
        }
    }
}
