// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract LargeDivRem {
    function rem_2(int n) public pure returns (int q) {
        assembly {
            q := smod(n, 2)
        }
    }

    function div_2(int n) public pure returns (int q) {
        assembly {
            q := sdiv(n, 2)
        }
    }

    function rem_7(int n) public pure returns (int q) {
        assembly {
            q := smod(n, 7)
        }
    }

    function div_7(int n) public pure returns (int q) {
        assembly {
            q := sdiv(n, 2)
        }
    }

    function rem_k(int n, int k) public pure returns (int q) {
        assembly {
            q := smod(n, k)
        }
    }

    function div_k(int n, int k) public pure returns (int q) {
        assembly {
            q := sdiv(n, k)
        }
    }
}
