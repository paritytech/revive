// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Returns `calldatasize()` bytes from `not(0)`.
contract ZeroLengthExitFallback {
    fallback() external payable {
        assembly {
            return(not(0), calldatasize())
        }
    }
}
