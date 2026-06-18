// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

/// Probe: less-common panic codes' revert data must match EVM.
contract PanicCodes {
    enum E { A, B, C }
    function enumConv(uint256 x) external pure returns (E) { return E(x); }            // 0x21 if x>2
    function uninitFp() external pure returns (uint256) {
        function(uint256) internal pure returns (uint256) fp;
        return fp(5);                                                                   // 0x51
    }
    function overAlloc(uint256 n) external pure returns (uint256) {
        uint256[] memory a = new uint256[](n);                                          // 0x41 if huge
        return a.length;
    }
    function shiftEdge(uint256 a, uint256 s) external pure returns (uint256) {
        return a << s;                                                                  // no panic, edge
    }
}
