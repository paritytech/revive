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
                        "contract": "SAR"
                    }
                }
            }
        }
    ]
}
*/

contract SAR {
  constructor() payable {
    assert(sar(0x03, 0x01) == 0x01);
    assert(
      sar(
        0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff,
        0x01
      ) == 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
    );
    assert(
      sar(
        0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff,
        0xff
      ) == 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
    );
    assert(
      sar(
        0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff,
        0x100
      ) == 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
    );
  }

  function sar(uint256 a, uint256 b) public pure returns (uint256 c) {
    assembly {
      c := sar(b, a)
    }
  }
}
