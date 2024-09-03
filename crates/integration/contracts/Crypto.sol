// SPDX-License-Identifier: MIT

pragma solidity ^0.8.24;

/* runner.json
{
    "actions": [
    {
      "Instantiate": {}
    },
    {
      "Call": {
        "dest": {
            "Instantiated": 0
        },
        "data": "f9fbd5540000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000c68656c6c6f20776f726c64210000000000000000000000000000000000000000"
      }
    },
    {
        "VerifyCall": {
            "success": true,
            "output": "57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6"
        }
    }
  ]
}
*/

contract TestSha3 {
    function test(string memory _pre) external payable returns (bytes32 hash) {
        hash = keccak256(bytes(_pre));
    }
}
