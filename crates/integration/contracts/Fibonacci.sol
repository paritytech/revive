// SPDX-License-Identifier: UNLICENSED

pragma solidity ^0.8;

// https://medium.com/coinmonks/fibonacci-in-solidity-8477d907e22a

contract FibonacciRecursive {
    function f(uint n) internal pure returns (uint) {
        if (n <= 1) {
            return n;
        } else {
            return f(n - 1) + f(n - 2);
        }
    }

    function fib3(uint n) public pure returns (uint) {
        return f(n);
    }
}

contract FibonacciIterative {
    function fib3(uint n) external pure returns (uint b) {
        if (n == 0) {
            return 0;
        }
        uint a = 1;
        b = 1;
        for (uint i = 2; i < n; i++) {
            unchecked {
                uint c = a + b;
                a = b;
                b = c;
            }
        }
        return b;
    }
}

contract FibonacciBinet {
    function fib3(uint n) external pure returns (uint a) {
        if (n == 0) {
            return 0;
        }
        uint h = n / 2;
        uint mask = 1;
        // find highest set bit in n
        while (mask <= h) {
            mask <<= 1;
        }
        mask >>= 1;
        a = 1;
        uint b = 1;
        uint c;
        while (mask > 0) {
            c = a * a + b * b;
            if (n & mask > 0) {
                b = b * (b + 2 * a);
                a = c;
            } else {
                a = a * (2 * b - a);
                b = c;
            }
            mask >>= 1;
        }
        return a;
    }
}
