// SPDX-License-Identifier: MIT

pragma solidity ^0.8.35;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "ERC7201"
                    }
                }
            }
        }
    ]
}
*/

/// Tests the `erc7201` builtin introduced in solc 0.8.35.
/// Reference: https://eips.ethereum.org/EIPS/eip-7201
contract ERC7201 {
    /// Reference implementation of the ERC-7201 base slot formula in pure
    /// Solidity. Used to cross-check the builtin output.
    function referenceSlot(string memory id) internal pure returns (uint256) {
        return uint256(
            keccak256(bytes.concat(bytes32(uint256(keccak256(bytes(id))) - 1)))
                & ~bytes32(uint256(0xff))
        );
    }

    constructor() payable {
        // Compile-time path: literal argument, solc folds the result into a
        // numeric constant in the emitted Yul.
        assert(erc7201("example.main") == referenceSlot("example.main"));
        assert(
            erc7201("erc7201:openzeppelin.storage.Initializable")
                == referenceSlot("erc7201:openzeppelin.storage.Initializable")
        );
        assert(erc7201("") == referenceSlot(""));

        // Runtime path: non-literal argument, solc emits a Yul utility
        // function that performs the keccak256 hashes at runtime.
        string memory id = "example.main";
        assert(erc7201(id) == referenceSlot(id));

        string memory empty = "";
        assert(erc7201(empty) == referenceSlot(empty));

        // The two paths must agree.
        assert(erc7201("example.main") == erc7201(id));
    }
}
