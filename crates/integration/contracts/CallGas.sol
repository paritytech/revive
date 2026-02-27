// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

// Use a non-zero call gas that works with call gas clipping but not with a truncate. 

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Upload": {
                "code": {
                    "Solidity": {
                        "contract": "Other"
                    }
                }
            }
        },
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "CallGas"
                    }
                },
                "data": "1000000000000000000000000000000000000000000000000000000000000001"
            }
        }
    ]
}
*/

contract Other {
    address public last;
    uint public foo;

    fallback() external {
        last = msg.sender; 
        foo += 1;
    }
}

contract CallGas {
    constructor(uint _gas) payable {
        Other other = new Other();
        address(other).call{ gas: _gas }(hex"");
        assert(other.last() == address(this));
    }
}
