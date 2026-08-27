// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

contract RecursiveUint128Arithmetic {
    function step(uint128 a, uint128 b, uint128 c, uint128 d, uint128 remaining)
        internal
        pure
        returns (uint128)
    {
        unchecked {
            if (remaining == 0) {
                return a ^ b ^ c ^ d;
            }
            return step(a * b + c, b * c + d, c * d + a, d * a + b, remaining - 1);
        }
    }

    function combine(uint128 a, uint128 b, uint128 c, uint128 d, uint128 rounds)
        external
        pure
        returns (uint128)
    {
        return step(a, b, c, d, rounds);
    }
}
