/// Both-constant DIV variants. The selector index is read from calldata[0..32]
/// and dispatched to a branch with two literal operands. Compiled directly via
/// revive's Yul-to-LLVM path (bypassing solc's Yul optimizer) so the literals
/// reach LLVM intact and exercise revive's "two literal operands" codegen path.
object "DivBothConst" {
    code {
        let size := datasize("DivBothConst_deployed")
        codecopy(0, dataoffset("DivBothConst_deployed"), size)
        return(0, size)
    }
    object "DivBothConst_deployed" {
        code {
            let which := calldataload(0)
            // `tag` is XORed into the result so that any poison/undef LLVM
            // produces from a UB-triggering const-fold propagates and diverges
            // from EVM, instead of coincidentally returning 0 and masking it.
            let tag := calldataload(32)
            let r := 0
            switch which
            case 0  { r := div(5, 5) }
            case 1  { r := div(5, 1) }
            case 2  { r := div(0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff) }
            case 3  { r := div(5, 2) }
            case 4  { r := div(1, 0) }
            default { revert(0, 0) }
            mstore(0, xor(r, tag))
            return(0, 32)
        }
    }
}
