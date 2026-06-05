// SPDX-License-Identifier: MIT

pragma solidity >=0.8.0;

/// Regression reproducer for a newyork SSA-validation ICE that surfaced only with
/// the solc optimizer disabled (`--disable-solc-optimizer`).
///
/// `try ... catch { return <constant>; }` lowers (with the solc optimizer off) to a
/// switch whose default region is `let r := if (true) { ...; leave } else { ... }`
/// followed by `yield r`. The inliner's leave-elimination produces the
/// constant-condition `if` with an output; the simplifier folds it, appending the
/// output binding after the branch's `leave`. The dead-code pass then truncated
/// everything after that terminator — including the binding the `yield` still
/// referenced — leaving `yield r` dangling ("value used before definition").
///
/// Both switch arms `leave`, so the yielded fall-through value is provably never
/// observed; the fix zero-binds the rescued value before the terminator so SSA
/// stays valid. Instantiated with the solc optimizer disabled to hit the path.
contract TryCatchCatchReturn {
    function boom() external pure returns (uint256) {
        revert("boom");
    }

    function run() public returns (uint256) {
        try this.boom() returns (uint256 value) {
            return value;
        } catch {
            return 200;
        }
    }
}
