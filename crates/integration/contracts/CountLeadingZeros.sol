// SPDX-License-Identifier: MIT

pragma solidity ^0.8.31;

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "CountLeadingZeros"
                    }
                }
            }
        }
    ]
}
*/

/// The EIP-7939 test vectors:
/// https://eips.ethereum.org/EIPS/eip-7939#test-cases
contract CountLeadingZeros {
    function clz(uint256 x) internal pure returns (uint256 r) {
        assembly {
            r := clz(x)
        }
    }

    constructor() payable {
        assert(
            clz(0x000000000000000000000000000000000000000000000000000000000000000)
                == 0x0000000000000000000000000000000000000000000000000000000000000100
        );
        assert(
            clz(0x8000000000000000000000000000000000000000000000000000000000000000)
                == 0x0000000000000000000000000000000000000000000000000000000000000000
        );
        assert(
            clz(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
                == 0x0000000000000000000000000000000000000000000000000000000000000000
        );
        assert(
            clz(0x4000000000000000000000000000000000000000000000000000000000000000)
                == 0x0000000000000000000000000000000000000000000000000000000000000001
        );
        assert(
            clz(0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
                == 0x0000000000000000000000000000000000000000000000000000000000000001
        );
        assert(
            clz(0x0000000000000000000000000000000000000000000000000000000000000001)
                == 0x00000000000000000000000000000000000000000000000000000000000000ff
        );
    }
}
