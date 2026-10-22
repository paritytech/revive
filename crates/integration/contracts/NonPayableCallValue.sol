// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract NonPayableCallValue {
    uint256 private a;
    mapping(uint256 => uint256) private credit;
    function _credit(uint256 k, uint256 n) internal {
        for (uint256 i = 0; i < n; i++) {
            credit[k + i] += msg.value;
        }
    }
    function setA(uint256 x) external returns (uint256) { a = x; return a; }
    function get(uint256 k) external view returns (uint256) { return credit[k] + a; }
    function depositA(uint256 n) external payable { _credit(a, n); }
    function depositB(uint256 n) external payable { _credit(a + 100, n); }
}
