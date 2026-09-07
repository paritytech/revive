// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Division, remainder, and mulmod by the SAME compile-time constant in one
/// contract, one function per constant magnitude class. Exercises the
/// constant-divisor code paths (LLVM folding, DivRemPairs, Barrett
/// specialization, and runtime routing all branch by constant size) and
/// guards the bug class where a compiler's constant-specialization pass
/// reuses a per-constant helper across optimization phases.
contract DivModMulmodConst {
    function mixThree(uint256 x, uint256 y) public pure returns (uint256 q, uint256 r, uint256 m) {
        q = x / 3;
        r = x % 3;
        m = mulmod(x, y, 3);
    }

    function mixTwoPow64PlusOne(uint256 x, uint256 y) public pure returns (uint256 q, uint256 r, uint256 m) {
        q = x / (2**64 + 1);
        r = x % (2**64 + 1);
        m = mulmod(x, y, 2**64 + 1);
    }

    function mixTwoPow128MinusOne(uint256 x, uint256 y) public pure returns (uint256 q, uint256 r, uint256 m) {
        q = x / (2**128 - 1);
        r = x % (2**128 - 1);
        m = mulmod(x, y, 2**128 - 1);
    }

    function mixTwoPow128PlusOne(uint256 x, uint256 y) public pure returns (uint256 q, uint256 r, uint256 m) {
        q = x / (2**128 + 1);
        r = x % (2**128 + 1);
        m = mulmod(x, y, 2**128 + 1);
    }

    function mixTwoPow200(uint256 x, uint256 y) public pure returns (uint256 q, uint256 r, uint256 m) {
        q = x / (2**200);
        r = x % (2**200);
        m = mulmod(x, y, 2**200);
    }

    /// 2**255: div/rem fold to shifts and the mulmod modulus is the
    /// reciprocal-overflow power of two, taking the inline mask rewrite.
    function mixTwoPow255(uint256 x, uint256 y) public pure returns (uint256 q, uint256 r, uint256 m) {
        q = x / (2**255);
        r = x % (2**255);
        m = mulmod(x, y, 2**255);
    }

    /// 2**255 - 1: 255 bits, the sharp mulmod eligibility boundary — div/rem
    /// are Barrett-eligible while mulmod is not Barrett-rewritten.
    function mixTwoPow255MinusOne(uint256 x, uint256 y) public pure returns (uint256 q, uint256 r, uint256 m) {
        q = x / (2**255 - 1);
        r = x % (2**255 - 1);
        m = mulmod(x, y, 2**255 - 1);
    }

    function mixTwoPow255PlusThree(uint256 x, uint256 y) public pure returns (uint256 q, uint256 r, uint256 m) {
        q = x / (2**255 + 3);
        r = x % (2**255 + 3);
        m = mulmod(x, y, 2**255 + 3);
    }

    function mixMax(uint256 x, uint256 y) public pure returns (uint256 q, uint256 r, uint256 m) {
        q = x / type(uint256).max;
        r = x % type(uint256).max;
        m = mulmod(x, y, type(uint256).max);
    }
}
