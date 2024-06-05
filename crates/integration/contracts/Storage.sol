// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract Storage {
    function transient(uint value) public returns (uint ret) {
        assembly {
            let slot := 123
            tstore(slot, value)
            let success := call(0, 0, 0, 0, 0, 0, 0)
            ret := tload(slot)
        }
    }
}
