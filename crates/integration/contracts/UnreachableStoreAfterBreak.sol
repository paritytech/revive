// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract UnreachableStoreAfterBreak {
    fallback(bytes calldata) external returns (bytes memory) {
        while (true) {
            assembly { mstore(0, 7) }
            break;
            assembly { mstore(0, 9) }
        }
        uint256 r;
        assembly { r := mload(0) }
        return abi.encode(r);
    }
}
