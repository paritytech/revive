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
            "contract": "MemoryBounds"
          }
        }
      }
    },
    {
      "Call": {
        "dest": {
          "Instantiated": 0
        }
      }
    },
    {
      "Call": {
        "dest": {
          "Instantiated": 0
        },
        "data": "489eb33e00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001a300000000000000000000000000000000000000000000000000000000000000"
      }
    },
    {
      "Call": {
        "dest": {
          "Instantiated": 0
        },
        "data": "db1ff7bb00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001a300000000000000000000000000000000000000000000000000000000000000"
      }
    }
  ]
}
*/

contract MemoryBounds {
    function oobLoad(bytes memory data) public pure returns (uint256, bytes memory) {
        uint256 result1;
        assembly {
            let ptr := mload(add(data, 0x20))
            result1 := calldataload(ptr)
        }
        return (result1, data);
    }

    function oobCopy(bytes memory data) public pure returns (uint256, bytes memory) {
        assembly {
            calldatacopy(0, 0x3a0000000000000000000000000000000000, 0x20)
        }
    }

    fallback() external {
        assembly {
            // Accessing OOB offsets should always work when the length is 0.
            return(100000, 0)
        }
    }
}
