// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Reproducer from paritytech/bugbounty_reports#215.
contract FmpLoopMstore8 {
    function probe() external view returns (uint256 r) {
        assembly {
            for { let i := 0 } lt(i, 0x40) { i := add(i, 1) } { mstore8(add(0x30, i), 0xff) }
            pop(keccak256(0x80, 0x180))
            r := mload(0x40)
            mstore(0x40, 0x80)
        }
    }
}
