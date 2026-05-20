// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Soundness PoC: `mem_opt::optimize_statements` `*Copy` handler
/// (CodeCopy / ExtCodeCopy / ReturnDataCopy / DataCopy /
/// CallDataCopy) only invalidates `memory_state[word(dest)]` when
/// `dest` is statically known. It ignores the *length* entirely, so
/// additional words inside `[dest, dest + length)` keep their stale
/// tracked entries. A subsequent `mload` at the exact tracked offset
/// is forwarded to the pre-overwrite value while EVM reads the
/// actually-overwritten bytes from the copy.
///
/// The dynamic `length` parameter defeats solc's dead-store
/// elimination of the sentinel — solc can't prove that
/// `calldatacopy(0xc0, 0, length)` overwrites bytes `0xe0..0x100`
/// without knowing `length >= 0x40`.
contract CopyOverlapBug {
    function bug(uint256 length) external pure returns (bytes32 r) {
        assembly {
            mstore(0xe0, 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa)
            calldatacopy(0xc0, 0, length)
            r := mload(0xe0)
        }
    }
}
