// SPDX-License-Identifier: MIT
pragma solidity ^0.8;

contract ArrayProbe {
    uint256[8] fixedArr;
    uint256[] dynArr;
    mapping(uint256 => uint256) m;

    function run(uint256 a, uint256 b, uint256 c) public returns (uint256) {
        for (uint256 i = 0; i < 8; i++) { fixedArr[i] = i * a + 1; }
        // attacker-controlled index: out-of-bounds must revert (panic 0x32) identically
        uint256 r = fixedArr[a & 7];
        r = r + fixedArr[b & 7];
        // dynamic array push/index
        uint256 n = c & 15;
        for (uint256 i = 0; i < n; i++) { dynArr.push(i + a); }
        if (dynArr.length > 0) { r = r ^ dynArr[b % dynArr.length]; }
        // mapping
        m[a] = b;
        m[b] = c;
        r = r + m[a] + m[b];
        // explicit bounds hit (reverts when a >= 8)
        r = r + fixedArr[a];
        return r;
    }
}
