// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;
contract DbgScope {
  function run() external {
    assembly {
      function f() { }
      let q := msize()
    }
  }
}
