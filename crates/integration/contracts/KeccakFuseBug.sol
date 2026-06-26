// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract KeccakFuseBug {
    address public target;

    constructor() {
        target = address(this);
    }

    function probe(uint256[8] calldata seeds)
        external view returns (uint256 r, bytes32 sink_out)
    {
        bytes32 sink;
        uint256 s0 = seeds[0]; uint256 s1 = seeds[1]; uint256 s2 = seeds[2];
        uint256 s3 = seeds[3]; uint256 s4 = seeds[4]; uint256 s5 = seeds[5];
        uint256 s6 = seeds[6]; uint256 s7 = seeds[7];
        assembly {
            mstore(0, s0) sink := xor(sink, keccak256(0, 32))
            mstore(0, s1) sink := xor(sink, keccak256(0, 32))
            mstore(0, s2) sink := xor(sink, keccak256(0, 32))
            mstore(0, s3) sink := xor(sink, keccak256(0, 32))
            mstore(0, s4) sink := xor(sink, keccak256(0, 32))
            mstore(0, s5) sink := xor(sink, keccak256(0, 32))
            mstore(0, s6) sink := xor(sink, keccak256(0, 32))
            mstore(0, s7) sink := xor(sink, keccak256(0, 32))
        }
        (bool ok,) = target.staticcall("");
        ok;
        assembly {
            r := mload(0)
        }
        sink_out = sink;
    }
}
