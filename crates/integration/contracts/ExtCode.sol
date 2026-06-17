// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract ExtCode {
    function ExtCodeSize(address who) public view returns (uint ret) {
        assembly {
            ret := extcodesize(who)
        }
    }

    // Two distinct addresses queried in a single function body: regression guard
    // for the `code_size` import memory-attribute bug where LLVM CSE'd the two
    // syscalls into one and returned `2 * extcodesize(a)`.
    function ExtCodeSizeSum(address a, address b) public view returns (uint ret) {
        assembly {
            ret := add(extcodesize(a), extcodesize(b))
        }
    }

    function CodeSize() public pure returns (uint ret) {
        assembly {
            ret := codesize()
        }
    }

    function ExtCodeHash(address who) public view returns (bytes32 ret) {
        assembly {
            ret := extcodehash(who)
        }
    }

    function CodeHash() public view returns (bytes32 ret) {
        assembly {
            ret := extcodehash(address())
        }
    }
}
