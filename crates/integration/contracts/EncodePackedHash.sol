// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

/// Probe: abi.encodePacked tight-packing of mixed-width values + keccak256
/// (signature/CREATE2/mapping-key hashing). Must match EVM exactly.
contract EncodePackedHash {
    function h(uint8 a, uint256 b, address c, uint16 d, bytes memory e) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, b, c, d, e));
    }
    function h2(uint256 a, uint256 b) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, b));
    }
    function h3(string memory s, uint8 n) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(s, n));
    }
}
