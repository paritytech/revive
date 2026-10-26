// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract PartialRangeCheck {
    function requireByteUnlessLarge(uint256 value) internal pure {
        if (value > 1000) return;
        require(value == uint8(value));
    }

    fallback(bytes calldata input) external returns (bytes memory) {
        uint256 value = abi.decode(input, (uint256));
        requireByteUnlessLarge(value);
        return abi.encode(value);
    }
}
