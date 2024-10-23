// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

/* runner.json
{
  "differential": true,
  "actions": [
    {
      "Upload": {
        "code": {
          "Solidity": {
            "contract": "ImmutablesTester"
          }
        }
      }
    },
    {
      "Instantiate": {
        "code": {
          "Solidity": {
            "contract": "Immutables"
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
    }
  ]
}
*/

contract ImmutablesTester {
    // Read should work in the runtime code
    uint public immutable foo;
    // Read should work in the runtime code
    uint public immutable bar;
    // Read should work in the runtime code
    uint public immutable zoo;

    // Assign and read should work in the constructor
    constructor(uint _foo) payable {
        foo = _foo;
        bar = foo + 1;
        zoo = bar + 2;

        assert(zoo == _foo + 3);
    }
}

contract Immutables {
    fallback() external {
        ImmutablesTester tester = new ImmutablesTester(127);

        assert(tester.foo() == 127);
        assert(tester.bar() == tester.foo() + 1);
        assert(tester.zoo() == tester.bar() + 2);
    }
}
