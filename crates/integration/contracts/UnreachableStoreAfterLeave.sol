// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract UnreachableStoreAfterLeave {
    function store() internal pure {
        while (true) {
            assembly { mstore(0, 7) }
            return;
            assembly { mstore(0, 9) }
        }
    }

    fallback(bytes calldata) external returns (bytes memory) {
        store();
        uint256 r;
        assembly { r := mload(0) }
        return abi.encode(r);
    }
}
