// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract DivisionArithmetics {
    function div(uint n, uint d) public pure returns (uint q) {
        assembly {
            q := div(n, d)
        }
    }

    function sdiv(int n, int d) public pure returns (int q) {
        assembly {
            q := sdiv(n, d)
        }
    }

    function mod(uint n, uint d) public pure returns (uint r) {
        assembly {
            r := mod(n, d)
        }
    }

    function smod(int n, int d) public pure returns (int r) {
        assembly {
            r := smod(n, d)
        }
    }

    /// Regression / soundness PoC: newyork simplifier folds `div(x, x) → 1`
    /// and `sdiv(x, x) → 1` for any runtime value `x`, but EVM defines
    /// division by zero to return 0, so `div(0, 0) = 0`. The fold is
    /// unsound when `x == 0`.
    ///
    /// `mod(x, x) → 0` is sound because EVM also returns 0 for mod by zero.
    function divSelf(uint x) external pure returns (uint r) {
        assembly { r := div(x, x) }
    }
    function sdivSelf(int x) external pure returns (int r) {
        assembly { r := sdiv(x, x) }
    }
    function modSelf(uint x) external pure returns (uint r) {
        assembly { r := mod(x, x) }
    }
}
