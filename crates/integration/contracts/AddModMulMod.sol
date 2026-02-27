// SPDX-License-Identifier: GPL-3.0

pragma solidity ^0.8;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Upload": {
                "code": {
                    "Solidity": {
                        "contract": "AddModMulMod"
                    }
                }
            }
        },
        {
            "Instantiate": {
                "value": 123123,
                "code": {
                    "Solidity": {
                        "contract": "AddModMulModTester"
                    }
                }
            }
        },
        {
            "VerifyCall": {
                "success": true
            }
        }
    ]
}
*/

contract AddModMulMod {
    function test() public returns (uint256) {
        // Note that this only works because computation on literals is done using
        // unbounded integers.
        if ((2**255 + 2**255) % 7 != addmod(2**255, 2**255, 7)) return 1;
        if ((2**255 + 2**255) % 7 != addmod(2**255, 2**255, 7)) return 2;
        return 0;
    }

    function f(uint256 d) public pure returns (uint256) {
        addmod(1, 2, d);
        return 2;
    }

    function g(uint256 d) public pure returns (uint256) {
        mulmod(1, 2, d);
        return 2;
    }

    function h() public pure returns (uint256) {
        mulmod(0, 1, 2);
        mulmod(1, 0, 2);
        addmod(0, 1, 2);
        addmod(1, 0, 2);
        return 2;
    }
}

contract AddModMulModTester {
    constructor() payable {
        AddModMulMod c = new AddModMulMod();

        assert(c.test() == 0);

        try c.f(0) returns (uint m) { revert(); } catch Panic(uint errorCode) {
            assert(errorCode == 0x12);
        }

        try c.g(0) returns (uint m) { revert(); } catch Panic(uint errorCode) {
            assert(errorCode == 0x12);
        }

        assert(c.h() == 2);
    } 
}
