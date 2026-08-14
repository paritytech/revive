// SPDX-License-Identifier: MIT

pragma solidity >=0.8.29;

/// Soundness PoC (newyork keccak lowering): folding a constant-operand
/// `keccak256(0, 0x40)` to a literal hash removes the fused helper that
/// writes the hash inputs back to scratch `[0, 0x40)`, while the memory
/// optimizer already dead-eliminated the staging `mstore`s on the strength
/// of that write-back.
///
/// `m[7] = 0xBEEF` lowers to `mstore(0, 7); mstore(0x20, 0);
/// sstore(keccak256(0, 0x40), 0xBEEF)`. The `staticcall` is a
/// load-forwarding barrier, so the trailing `mload(0)` must observe the
/// hashed key `7` from memory. EVM and the stock pipeline return 7; the
/// newyork const-fold path leaves scratch unwritten and returns 0.
contract KeccakConstFoldScratchBug {
    mapping(uint256 => uint256) m;

    function run() external returns (uint256 leftover) {
        m[7] = 0xBEEF;
        address(0x04).staticcall("");
        assembly {
            leftover := mload(0)
        }
    }
}
