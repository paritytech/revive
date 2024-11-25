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
                        "contract": "Logic"
                    }
                }
            }
        },
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "Tester"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "value": 123,
                "data": "6466414b0000000000000000000000000000000000000000000000000000000000000020"
            }
        }
    ]
}
*/

contract Logic {
    // NOTE: storage layout must be the same as contract Tester
    uint256 public num;
    address public sender;
    uint256 public value;

    uint public immutable multiplier = 4;

    event DidSetVars();

    function setVars(uint256 _num) public payable returns (uint256) {
        num = _num * multiplier;
        sender = msg.sender;
        value = msg.value;
        emit DidSetVars();
        return _num;
    }
}

contract Tester {
    uint256 public num;
    address public sender;
    uint256 public value;

    uint public immutable multiplier = 2;

    function setVars(uint256 _num) public payable returns (bool, bytes memory) {
        Logic impl = new Logic();

        // Tester's storage is set, Logic is not modified.
        (bool success, bytes memory data) = address(impl).delegatecall(
            abi.encodeWithSignature("setVars(uint256)", _num)
        );

        assert(success);
        assert(impl.num() == 0);
        assert(impl.sender() == address(0));
        assert(impl.value() == 0);
        assert(num == _num * 4);
        assert(sender == msg.sender);
        assert(value == msg.value);

        return (success, data);
    }
}
