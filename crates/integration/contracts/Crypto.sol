// SPDX-License-Identifier: MIT

pragma solidity ^0.8.24;

/* runner.json
{
  "differential": true,
  "actions": [
    {
      "Instantiate": {
        "code": {
          "Solidity": {
            "contract": "TestSha3"
          }
        }
      }
    },
    {
      "Call": {
        "dest": {
          "Instantiated": 0
        },
        "data": "f9fbd5540000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000c68656c6c6f20776f726c64210000000000000000000000000000000000000000"
      }
    }
  ]
}
*/

contract TestSha3 {
    function test(string memory _pre) external payable returns (bytes32) {
        bytes32 hash = keccak256(bytes(_pre));
        return bytes32(uint(hash) + 1);
    }
}
