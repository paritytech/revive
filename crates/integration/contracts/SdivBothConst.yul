/// Both-constant SDIV variants. Selector index is read from calldata[0..32]
/// and dispatched to a branch with two literal operands. Compiled directly via
/// revive's Yul-to-LLVM path (bypassing solc's Yul optimizer) so the literals
/// reach LLVM intact and exercise revive's "two literal operands" codegen path.
///
/// Case 11 — sdiv(INT_MIN, -1) — is the signed-overflow edge case that is UB
/// in LLVM `sdiv` and must be guarded in revive's codegen.
object "SdivBothConst" {
    code {
        let size := datasize("SdivBothConst_deployed")
        codecopy(0, dataoffset("SdivBothConst_deployed"), size)
        return(0, size)
    }
    object "SdivBothConst_deployed" {
        code {
            let which := calldataload(0)
            // `tag` is XORed into the result so that any poison/undef LLVM
            // produces from a UB-triggering const-fold (e.g. sdiv(INT_MIN,-1))
            // propagates and diverges from EVM, instead of coincidentally
            // returning 0 and masking the bug.
            let tag := calldataload(32)
            let r := 0
            switch which
            case 0  { r := sdiv(5, 5) }
            case 1  { r := sdiv(5, 1) }
            case 2  { r := sdiv(0, 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff) }
            case 3  { r := sdiv(0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff) }
            case 4  { r := sdiv(5, 2) }
            case 5  { r := sdiv(5, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff) }
            case 6  { r := sdiv(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe) }
            case 7  { r := sdiv(0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb) }
            case 8  { r := sdiv(0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb, 2) }
            case 9  { r := sdiv(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0x8000000000000000000000000000000000000000000000000000000000000000) }
            case 10 { r := sdiv(1, 0) }
            case 11 { r := sdiv(0x8000000000000000000000000000000000000000000000000000000000000000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff) }
            case 12 { r := sdiv(0x8000000000000000000000000000000000000000000000000000000000000001, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff) }
            default { revert(0, 0) }
            mstore(0, xor(r, tag))
            return(0, 32)
        }
    }
}
