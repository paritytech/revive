// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Reproducer from paritytech/bugbounty_reports#214.
contract FmpLiteralNotZero {
    function probe() external view returns (uint256 r) {
        assembly {
            mstore(0x40, not(0))
            mstore(add(0x2000, calldatasize()), 0)
            r := mload(0x40)
            mstore(0x40, 0x80)
        }
    }
}
