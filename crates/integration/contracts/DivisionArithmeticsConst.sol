// SPDX-License-Identifier: MIT

pragma solidity ^0.8;

/// Half-constant variants of the operations in `DivisionArithmetics.sol`:
/// one operand reaches the compiler as a literal, the other comes from
/// calldata. solc cannot fold an op when one side is a parameter, so the
/// final bytecode contains a real div/sdiv/mod/smod with one constant
/// operand — exercising revive's "one literal operand" codegen path.
///
/// Split into four contracts (one per opcode) so each compiled blob stays
/// well under polkavm-linker's debug line-program limit. A single 58-function
/// contract overflowed it in debug builds.
///
/// Two-constant cases are covered by Yul fixtures (see
/// `*BothConst.yul`) to bypass solc's Yul optimizer.

contract DivConst {
    function divRhsZero(uint256 n) public pure returns (uint256 r) { assembly { r := div(n, 0) } }
    function divRhsOne(uint256 n) public pure returns (uint256 r) { assembly { r := div(n, 1) } }
    function divRhsTwo(uint256 n) public pure returns (uint256 r) { assembly { r := div(n, 2) } }
    function divRhsFive(uint256 n) public pure returns (uint256 r) { assembly { r := div(n, 5) } }
    function divRhsMax(uint256 n) public pure returns (uint256 r) {
        uint256 c = type(uint256).max;
        assembly { r := div(n, c) }
    }

    function divLhsZero(uint256 d) public pure returns (uint256 r) { assembly { r := div(0, d) } }
    function divLhsOne(uint256 d) public pure returns (uint256 r) { assembly { r := div(1, d) } }
    function divLhsTwo(uint256 d) public pure returns (uint256 r) { assembly { r := div(2, d) } }
    function divLhsFive(uint256 d) public pure returns (uint256 r) { assembly { r := div(5, d) } }
    function divLhsMax(uint256 d) public pure returns (uint256 r) {
        uint256 c = type(uint256).max;
        assembly { r := div(c, d) }
    }
}

contract SdivConst {
    function sdivRhsZero(int256 n) public pure returns (int256 r) { assembly { r := sdiv(n, 0) } }
    function sdivRhsOne(int256 n) public pure returns (int256 r) { assembly { r := sdiv(n, 1) } }
    function sdivRhsNegOne(int256 n) public pure returns (int256 r) {
        int256 c = -1;
        assembly { r := sdiv(n, c) }
    }
    function sdivRhsTwo(int256 n) public pure returns (int256 r) { assembly { r := sdiv(n, 2) } }
    function sdivRhsNegTwo(int256 n) public pure returns (int256 r) {
        int256 c = -2;
        assembly { r := sdiv(n, c) }
    }
    function sdivRhsFive(int256 n) public pure returns (int256 r) { assembly { r := sdiv(n, 5) } }
    function sdivRhsNegFive(int256 n) public pure returns (int256 r) {
        int256 c = -5;
        assembly { r := sdiv(n, c) }
    }
    function sdivRhsMin(int256 n) public pure returns (int256 r) {
        int256 c = type(int256).min;
        assembly { r := sdiv(n, c) }
    }
    function sdivRhsMinPlusOne(int256 n) public pure returns (int256 r) {
        int256 c = type(int256).min + 1;
        assembly { r := sdiv(n, c) }
    }
    function sdivRhsMax(int256 n) public pure returns (int256 r) {
        int256 c = type(int256).max;
        assembly { r := sdiv(n, c) }
    }

    function sdivLhsZero(int256 d) public pure returns (int256 r) { assembly { r := sdiv(0, d) } }
    function sdivLhsOne(int256 d) public pure returns (int256 r) { assembly { r := sdiv(1, d) } }
    function sdivLhsNegOne(int256 d) public pure returns (int256 r) {
        int256 c = -1;
        assembly { r := sdiv(c, d) }
    }
    function sdivLhsTwo(int256 d) public pure returns (int256 r) { assembly { r := sdiv(2, d) } }
    function sdivLhsNegTwo(int256 d) public pure returns (int256 r) {
        int256 c = -2;
        assembly { r := sdiv(c, d) }
    }
    function sdivLhsFive(int256 d) public pure returns (int256 r) { assembly { r := sdiv(5, d) } }
    function sdivLhsNegFive(int256 d) public pure returns (int256 r) {
        int256 c = -5;
        assembly { r := sdiv(c, d) }
    }
    function sdivLhsMin(int256 d) public pure returns (int256 r) {
        int256 c = type(int256).min;
        assembly { r := sdiv(c, d) }
    }
    function sdivLhsMinPlusOne(int256 d) public pure returns (int256 r) {
        int256 c = type(int256).min + 1;
        assembly { r := sdiv(c, d) }
    }
    function sdivLhsMax(int256 d) public pure returns (int256 r) {
        int256 c = type(int256).max;
        assembly { r := sdiv(c, d) }
    }
}

contract ModConst {
    function modRhsZero(uint256 n) public pure returns (uint256 r) { assembly { r := mod(n, 0) } }
    function modRhsOne(uint256 n) public pure returns (uint256 r) { assembly { r := mod(n, 1) } }
    function modRhsTwo(uint256 n) public pure returns (uint256 r) { assembly { r := mod(n, 2) } }
    function modRhsFive(uint256 n) public pure returns (uint256 r) { assembly { r := mod(n, 5) } }
    function modRhsMax(uint256 n) public pure returns (uint256 r) {
        uint256 c = type(uint256).max;
        assembly { r := mod(n, c) }
    }

    function modLhsZero(uint256 d) public pure returns (uint256 r) { assembly { r := mod(0, d) } }
    function modLhsOne(uint256 d) public pure returns (uint256 r) { assembly { r := mod(1, d) } }
    function modLhsTwo(uint256 d) public pure returns (uint256 r) { assembly { r := mod(2, d) } }
    function modLhsFive(uint256 d) public pure returns (uint256 r) { assembly { r := mod(5, d) } }
    function modLhsMax(uint256 d) public pure returns (uint256 r) {
        uint256 c = type(uint256).max;
        assembly { r := mod(c, d) }
    }
}

contract SmodConst {
    function smodRhsZero(int256 n) public pure returns (int256 r) { assembly { r := smod(n, 0) } }
    function smodRhsOne(int256 n) public pure returns (int256 r) { assembly { r := smod(n, 1) } }
    function smodRhsNegOne(int256 n) public pure returns (int256 r) {
        int256 c = -1;
        assembly { r := smod(n, c) }
    }
    function smodRhsTwo(int256 n) public pure returns (int256 r) { assembly { r := smod(n, 2) } }
    function smodRhsNegTwo(int256 n) public pure returns (int256 r) {
        int256 c = -2;
        assembly { r := smod(n, c) }
    }
    function smodRhsFive(int256 n) public pure returns (int256 r) { assembly { r := smod(n, 5) } }
    function smodRhsNegFive(int256 n) public pure returns (int256 r) {
        int256 c = -5;
        assembly { r := smod(n, c) }
    }
    function smodRhsMin(int256 n) public pure returns (int256 r) {
        int256 c = type(int256).min;
        assembly { r := smod(n, c) }
    }
    function smodRhsMax(int256 n) public pure returns (int256 r) {
        int256 c = type(int256).max;
        assembly { r := smod(n, c) }
    }

    function smodLhsZero(int256 d) public pure returns (int256 r) { assembly { r := smod(0, d) } }
    function smodLhsOne(int256 d) public pure returns (int256 r) { assembly { r := smod(1, d) } }
    function smodLhsNegOne(int256 d) public pure returns (int256 r) {
        int256 c = -1;
        assembly { r := smod(c, d) }
    }
    function smodLhsTwo(int256 d) public pure returns (int256 r) { assembly { r := smod(2, d) } }
    function smodLhsNegTwo(int256 d) public pure returns (int256 r) {
        int256 c = -2;
        assembly { r := smod(c, d) }
    }
    function smodLhsFive(int256 d) public pure returns (int256 r) { assembly { r := smod(5, d) } }
    function smodLhsNegFive(int256 d) public pure returns (int256 r) {
        int256 c = -5;
        assembly { r := smod(c, d) }
    }
    function smodLhsMin(int256 d) public pure returns (int256 r) {
        int256 c = type(int256).min;
        assembly { r := smod(c, d) }
    }
    function smodLhsMax(int256 d) public pure returns (int256 r) {
        int256 c = type(int256).max;
        assembly { r := smod(c, d) }
    }
}
