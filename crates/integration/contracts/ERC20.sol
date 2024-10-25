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
                    "contract": "ERC20"
                }
            }
        }
    },
    {
        "Call": {
            "dest": {
                "Instantiated": 0
            },
            "data": "313ce567"
        }
    },
    {
        "Call": {
            "dest": {
                "Instantiated": 0
            },
            "data": "06fdde03"
        }
    },
    {
        "Call": {
            "dest": {
                "Instantiated": 0
            },
            "data": "95d89b41"
        }
    },
    {
        "Call": {
            "dest": {
                "Instantiated": 0
            },
            "data": "a0712d680000000000000000000000000000000000000000000000000000000000003039"
        }
    },
    {
        "Call": {
            "dest": {
                "Instantiated": 0
            },
            "data": "70a082310000000000000000000000000101010101010101010101010101010101010101"
        }
    },
    {
        "Call": {
            "dest": {
                "Instantiated": 0
            },
            "data": "a9059cbb000000000000000000000000ed27012c24fda47a661de241c4030ecb9d18a76d000000000000000000000000000000000000000000000000000000000000007b"
        }
    },
    {
        "Call": {
            "dest": {
                "Instantiated": 0
            },
            "data": "095ea7b3000000000000000000000000ed27012c24fda47a661de241c4030ecb9d18a76d00000000000000000000000000000000000000000000000000000000000000ff"
        }
    },
    {
        "Call": {
            "dest": {
                "Instantiated": 0
            },
            "data": "dd62ed3e0000000000000000000000000101010101010101010101010101010101010101000000000000000000000000ed27012c24fda47a661de241c4030ecb9d18a76d"
        }
    }
  ]
}
*/

// https://github.com/OpenZeppelin/openzeppelin-contracts/blob/v3.0.0/contracts/token/ERC20/IERC20.sol
interface IERC20 {
    function totalSupply() external view returns (uint);

    function balanceOf(address account) external view returns (uint);

    function transfer(address recipient, uint amount) external returns (bool);

    function allowance(
        address owner,
        address spender
    ) external view returns (uint);

    function approve(address spender, uint amount) external returns (bool);

    function transferFrom(
        address sender,
        address recipient,
        uint amount
    ) external returns (bool);

    event Transfer(address indexed from, address indexed to, uint value);
    event Approval(address indexed owner, address indexed spender, uint value);
}

contract ERC20 is IERC20 {
    uint public totalSupply;
    mapping(address => uint) public balanceOf;
    mapping(address => mapping(address => uint)) public allowance;
    string public name = "Solidity by Example";
    string public symbol = "SOLBYEX";
    uint8 public decimals = 18;

    function transfer(address recipient, uint amount) external returns (bool) {
        balanceOf[msg.sender] -= amount;
        balanceOf[recipient] += amount;
        emit Transfer(msg.sender, recipient, amount);
        return true;
    }

    function approve(address spender, uint amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
        return true;
    }

    function transferFrom(
        address sender,
        address recipient,
        uint amount
    ) external returns (bool) {
        allowance[sender][msg.sender] -= amount;
        balanceOf[sender] -= amount;
        balanceOf[recipient] += amount;
        emit Transfer(sender, recipient, amount);
        return true;
    }

    function mint(uint amount) external {
        balanceOf[msg.sender] += amount;
        totalSupply += amount;
        emit Transfer(address(0), msg.sender, amount);
    }

    function burn(uint amount) external {
        balanceOf[msg.sender] -= amount;
        totalSupply -= amount;
        emit Transfer(msg.sender, address(0), amount);
    }
}
