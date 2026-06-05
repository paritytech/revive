/// Both-constant SMOD variants. Selector index is read from calldata[0..32]
/// and dispatched to a branch with two literal operands. Compiled directly via
/// revive's Yul-to-LLVM path (bypassing solc's Yul optimizer) so the literals
/// reach LLVM intact and exercise revive's "two literal operands" codegen path.
///
/// Case 15 — smod(INT_MIN, -1) — surfaces the bug reported in
/// paritytech/revive#524: LLVM constant-folds `srem(INT_MIN, -1)` to poison
/// because the operation is signed-overflow UB in LLVM, even though EVM SMOD
/// defines it as 0.
object "SmodBothConst" {
    code {
        let size := datasize("SmodBothConst_deployed")
        codecopy(0, dataoffset("SmodBothConst_deployed"), size)
        return(0, size)
    }
    object "SmodBothConst_deployed" {
        code {
            let which := calldataload(0)
            // `tag` is XORed into every result so that any poison/undef LLVM
            // produces from a UB-triggering const-fold propagates and diverges
            // from EVM, rather than coincidentally collapsing to 0 (which is
            // also EVM SMOD's defined result for INT_MIN op -1).
            let tag := calldataload(32)
            switch which
            case 0  { mstore(0, xor(smod(5, 5), tag)) }
            case 1  { mstore(0, xor(smod(5, 1), tag)) }
            case 2  { mstore(0, xor(smod(0, 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), tag)) }
            case 3  { mstore(0, xor(smod(0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), tag)) }
            case 4  { mstore(0, xor(smod(5, 2), tag)) }
            case 5  { mstore(0, xor(smod(2, 5), tag)) }
            case 6  { mstore(0, xor(smod(5, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb), tag)) }
            case 7  { mstore(0, xor(smod(5, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), tag)) }
            case 8  { mstore(0, xor(smod(5, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe), tag)) }
            case 9  { mstore(0, xor(smod(0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb, 2), tag)) }
            case 10 { mstore(0, xor(smod(0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 5), tag)) }
            case 11 { mstore(0, xor(smod(0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb), tag)) }
            case 12 { mstore(0, xor(smod(0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), tag)) }
            case 13 { mstore(0, xor(smod(0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe), tag)) }
            case 14 { mstore(0, xor(smod(0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb), tag)) }
            case 15 { mstore(0, xor(smod(0x8000000000000000000000000000000000000000000000000000000000000000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff), tag)) }
            case 16 { mstore(0, xor(smod(0, 0), tag)) }
            default { revert(0, 0) }
            return(0, 32)
        }
    }
}
