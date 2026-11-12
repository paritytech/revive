// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract KeccakPairScratchByteOrder {
    fallback() external {
        assembly {
            mstore(0, calldataload(0))
            if calldataload(0x40) { sstore(1, 1) }
            mstore(0x20, 7)
            sstore(0, keccak256(0, 0x40))
            for { let i := 0 } lt(i, calldataload(0x20)) { i := add(i, 1) } { mstore(0x80, i) }
            mstore(0x20, mload(0))
            return(0x20, 0x20)
        }
    }
}
contract KeccakSingleScratchByteOrder {
    fallback() external {
        assembly {
            mstore(0, calldataload(0))
            if calldataload(0x40) { sstore(1, 1) }
            sstore(0, keccak256(0, 0x20))
            for { let i := 0 } lt(i, calldataload(0x20)) { i := add(i, 1) } { mstore(0x80, i) }
            mstore(0x20, mload(0))
            return(0x20, 0x20)
        }
    }
}
contract MappingSLoadScratchByteOrder {
    fallback() external {
        assembly {
            mstore(0, calldataload(0))
            if calldataload(0x40) { sstore(1, 1) }
            mstore(0x20, 7)
            sstore(0, sload(keccak256(0, 0x40)))
            for { let i := 0 } lt(i, calldataload(0x20)) { i := add(i, 1) } { mstore(0x80, i) }
            mstore(0x20, mload(0))
            return(0x20, 0x20)
        }
    }
}
contract MappingSStoreScratchByteOrder {
    fallback() external {
        assembly {
            mstore(0, calldataload(0))
            if calldataload(0x40) { sstore(1, 1) }
            mstore(0x20, 7)
            sstore(keccak256(0, 0x40), 5)
            for { let i := 0 } lt(i, calldataload(0x20)) { i := add(i, 1) } { mstore(0x80, i) }
            mstore(0x20, mload(0))
            return(0x20, 0x20)
        }
    }
}
