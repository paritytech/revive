// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Storage references to mapping entries, returned from functions that are not inlined.
contract ReturnedStorageReference {
    struct S { uint256 a; uint256 b; }
    mapping(uint256 => S) m;

    /// Writes through the reference before returning it next to a second return value.
    function writeTwo(uint256 k, uint256 n) internal returns (uint256 c, S storage s) {
        if (n > 0) writeTwo(k, n - 1);
        s = m[k];
        s.a = 7;
        c = n + 1;
    }

    /// Reads through the reference before returning it.
    function read(uint256 k, uint256 n) internal view returns (S storage s, uint256 a) {
        if (n > 0) read(k, n - 1);
        s = m[k];
        a = s.a;
    }

    /// Mutually recursive with `odd`.
    function even(uint256 k, uint256 n) internal returns (S storage s) {
        if (n > 0) odd(k, n - 1);
        s = m[k];
        s.a = 5;
    }

    /// Mutually recursive with `even`.
    function odd(uint256 k, uint256 n) internal returns (S storage s) {
        if (n > 0) even(k, n - 1);
        s = m[k];
        s.a = 3;
    }

    /// Not recursive.
    function plain(uint256 k) internal returns (S storage s) {
        s = m[k];
        s.a = 11;
    }

    function runTwo(uint256 k, uint256 n) external returns (uint256, uint256, uint256) {
        (uint256 c, S storage s) = writeTwo(k, n);
        s.b = 9;
        return (c, m[k].a, m[k].b);
    }

    function runRead(uint256 k, uint256 n) external returns (uint256, uint256) {
        m[k].a = 13;
        (S storage s, uint256 a) = read(k, n);
        s.b = a + 1;
        return (a, m[k].b);
    }

    function runMutual(uint256 k, uint256 n) external returns (uint256, uint256) {
        S storage s = even(k, n);
        s.b = 9;
        return (m[k].a, m[k].b);
    }

    function runPlain(uint256 k) external returns (uint256, uint256) {
        S storage s = plain(k);
        s.b = 9;
        return (m[k].a, m[k].b);
    }
}
