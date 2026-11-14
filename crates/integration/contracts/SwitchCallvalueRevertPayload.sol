// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract SwitchCallvalueRevertPayload {
    fallback() external payable {
        assembly {
            let a := calldataload(0)
            let b := calldataload(32)
            switch calldataload(64)
            case 1 {
                if callvalue() { mstore(0, a) revert(0, 32) }
                mstore(0, add(a, b))
                return(0, 32)
            }
            case 2 {
                if callvalue() { mstore(0, b) revert(0, 32) }
                mstore(0, sub(a, b))
                return(0, 32)
            }
            default {
                if callvalue() { mstore(0, a) revert(0, 32) }
                mstore(0, mul(a, b))
                return(0, 32)
            }
        }
    }
}
