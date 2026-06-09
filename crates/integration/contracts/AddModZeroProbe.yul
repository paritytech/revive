/// Probe: `addmod(a, b, 0)` — LLVM `urem ..., 0` is UB; EVM defines ADDMOD
/// with modulus = 0 as 0. The stdlib `__addmod` runtime function guards this.
/// Question is whether LLVM constant-folds through the guard at O3.
object "AddModZeroProbe" {
    code {
        let size := datasize("AddModZeroProbe_deployed")
        codecopy(0, dataoffset("AddModZeroProbe_deployed"), size)
        return(0, size)
    }
    object "AddModZeroProbe_deployed" {
        code {
            let tag := calldataload(0)
            let a := calldataload(32)
            let b := calldataload(64)
            // 0 modulus, obfuscated so revive can't pre-fold
            let modulus := sub(shl(8, 1), shl(8, 1))
            mstore(0, xor(addmod(a, b, modulus), tag))
            return(0, 32)
        }
    }
}
