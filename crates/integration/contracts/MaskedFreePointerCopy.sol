// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract MaskedFreePointerCopy {
    fallback() external {
        assembly {
            mstore(0x40, and(calldataload(0x20), 0x1f))
            for { let i := 0 } lt(i, calldataload(0x40)) { i := add(i, 1) } { mstore(0x80, i) }
            calldatacopy(add(mload(0x40), 0x30), 0, 0x20)
            if iszero(gt(mload(0x40), 0xffffffff)) { revert(0, 0) }
        }
    }
}
