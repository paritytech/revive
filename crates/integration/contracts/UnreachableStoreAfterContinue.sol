// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract UnreachableStoreAfterContinue {
    fallback(bytes calldata) external returns (bytes memory) {
        for (uint256 i = 0; i < 1; i++) {
            assembly { mstore(0, 7) }
            continue;
            assembly { mstore(0, 9) }
        }
        uint256 r;
        assembly { r := mload(0) }
        return abi.encode(r);
    }
}
