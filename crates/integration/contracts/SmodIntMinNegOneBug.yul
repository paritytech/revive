/// Dedicated fixture for the `smod(INT_MIN, -1)` LLVM-UB bug reported in
/// paritytech/revive#524.
///
/// The bug is that LLVM constant-folds `srem(INT_MIN, -1)` to poison (signed
/// overflow UB), which then propagates through subsequent operations and
/// often collapses to 0. EVM SMOD defines this case as 0, so a bare
/// `smod(INT_MIN, -1)` would coincidentally agree; we XOR the result with a
/// runtime calldata word so any divergence between the (defined) EVM value
/// and the (poison-derived) PVM value becomes observable.
///
/// **Important compilation details that make the bug actually surface here:**
///
/// 1. Operands are written as Yul expressions (`shl(255, 1)`, `sar(58, shl(58,
///    sub(0,1)))`) rather than raw `0x8000...` / `0xfff...` literals. revive's
///    pipeline constant-evaluates raw literals before LLVM ever sees them, so
///    no `srem` instruction with both literal operands reaches LLVM and the
///    UB-fold opportunity vanishes. Yul expression operands force a real
///    `srem` to appear in LLVM IR.
///
/// 2. The function has a single execution path (no `switch` over many cases).
///    Multi-case dispatchers exceed LLVM's inlining budget for the runtime
///    `__revive_signed_remainder` function, so the CALL is preserved and
///    evaluated at runtime via RISC-V `rem` (which is defined as 0 for the
///    INT_MIN/-1 pair) — hiding the bug. With a single case the inliner
///    inlines aggressively, exposing the bug.
object "SmodIntMinNegOneBug" {
    code {
        let size := datasize("SmodIntMinNegOneBug_deployed")
        codecopy(0, dataoffset("SmodIntMinNegOneBug_deployed"), size)
        return(0, size)
    }
    object "SmodIntMinNegOneBug_deployed" {
        code {
            let tag := calldataload(0)
            let int_min := shl(255, 1)
            let neg_one := sar(58, shl(58, sub(0, 1)))
            mstore(0, xor(smod(int_min, neg_one), tag))
            return(0, 32)
        }
    }
}
