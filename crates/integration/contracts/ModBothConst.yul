/// Both-constant MOD variants. Selector index is read from calldata[0..32]
/// and dispatched to a branch with two literal operands. Compiled directly via
/// revive's Yul-to-LLVM path (bypassing solc's Yul optimizer) so the literals
/// reach LLVM intact and exercise revive's "two literal operands" codegen path.
object "ModBothConst" {
    code {
        let size := datasize("ModBothConst_deployed")
        codecopy(0, dataoffset("ModBothConst_deployed"), size)
        return(0, size)
    }
    object "ModBothConst_deployed" {
        code {
            let which := calldataload(0)
            // `tag` is XORed into the result so that any poison/undef LLVM
            // produces from a UB-triggering const-fold propagates and diverges
            // from EVM, instead of coincidentally returning 0 and masking it.
            let tag := calldataload(32)
            let r := 0
            switch which
            case 0 { r := mod(5, 5) }
            case 1 { r := mod(5, 1) }
            case 2 { r := mod(0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff) }
            case 3 { r := mod(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff) }
            case 4 { r := mod(5, 2) }
            case 5 { r := mod(2, 5) }
            case 6 { r := mod(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0) }
            default { revert(0, 0) }
            mstore(0, xor(r, tag))
            return(0, 32)
        }
    }
}
