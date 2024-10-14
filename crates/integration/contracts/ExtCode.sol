// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract ExtCode {
    function ExtCodeSize(address who) public view returns (uint ret) {
        assembly {
            ret := extcodesize(who)
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
