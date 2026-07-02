// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

// Regression test: contracts whose Yul IR generates internal helper functions
// (e.g. abi_encode_string, extract_byte_array_length) that survive the newyork
// inliner require a function scope in the LLVM context during code generation.
// Without it, declaring frontend functions in generate_object triggers an ICE:
// "function scope must be pushed before declaring frontend functions".

/* runner.json
{
    "differential": false,
    "actions": [
        {
            "Instantiate": {}
        },
        {
            "Call": {
                "dest": { "Instantiated": 0 },
                "data": "e942b51600000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000568656c6c6f0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005776f726c64000000000000000000000000000000000000000000000000000000"
            }
        },
        {
            "VerifyCall": {
                "success": true
            }
        },
        {
            "Call": {
                "dest": { "Instantiated": 0 },
                "data": "6d4ce63c"
            }
        },
        {
            "VerifyCall": {
                "success": true,
                "output": "00000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000568656c6c6f0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005776f726c64000000000000000000000000000000000000000000000000000000"
            }
        }
    ]
}
*/

contract InternalFn {
    string public a;
    string public b;

    function set(string calldata _a, string calldata _b) external {
        a = _a;
        b = _b;
    }

    function get() external view returns (string memory, string memory) {
        return (a, b);
    }
}
