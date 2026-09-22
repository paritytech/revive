// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Reproducer from paritytech/bugbounty_reports#219.
contract ZeroLengthExitFallback {
    fallback() external payable {
        assembly {
            return(not(0), calldatasize())
        }
    }
}
