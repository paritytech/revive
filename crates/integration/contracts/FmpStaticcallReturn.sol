// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Reproducer from paritytech/bugbounty_reports#216.
contract FmpStaticcallReturn {
    function probe() external view returns (uint256 r) {
        assembly {
            mstore(0x80, 0x100000000000000000000000000000000000000000000000007)
            pop(staticcall(gas(), 4, 0x80, 0x20, 0x40, 0x20))
            mstore(add(0x2000, calldatasize()), 0)
            r := mload(0x40)
            mstore(0x40, 0x80)
        }
    }
}
