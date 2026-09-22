// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Reproducer from paritytech/bugbounty_reports#216.
contract FmpMcopyStraddle {
    function probe() external view returns (uint256 r) {
        assembly {
            mstore(0x80, calldataload(0))
            mcopy(0x38, 0x80, 42)
            r := mload(0x40)
            mstore(0x40, 0x80)
        }
    }
}
