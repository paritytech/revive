// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

/* runner.json
{
  "differential": true,
  "actions": [
    {
      "Instantiate": {
	    "code": {
		  "Solidity": {
		    "contract": "Immutables"
		  }
		},
        "data": "000000000000000000000000000000000000000000000000000000000000007b"
      }
    },
    {
      "Call": {
        "dest": {
          "Instantiated": 0
        },
        "data": "c2985578"
      }
    },
    {
      "Call": {
        "dest": {
          "Instantiated": 0
        },
        "data": "febb0f7e"
      }
    },
    {
      "Call": {
        "dest": {
          "Instantiated": 0
        },
        "data": "7b6a8777"
      }
    },
    {
      "Call": {
        "dest": {
          "Instantiated": 0
        }
      }
    }
  ]
}
*/

contract Immutables {
    uint public immutable foo;
    uint public immutable bar;
    uint public immutable zoo;

    constructor(uint _foo) payable {
        foo = _foo;
        bar = foo + 1;
        zoo = bar + 2;
    }

    fallback() external {
        assert(foo > 0);
        assert(bar == foo + 1);
        assert(zoo == bar + 2);
    }
}
