// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.0;
import "solidity/contract.sol";
contract U {
    function u() external returns (address) { return address(new C()); }
}
