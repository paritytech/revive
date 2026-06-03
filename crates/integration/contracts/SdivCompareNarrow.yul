/// Soundness PoC: the under-estimated forward width of `sdiv` (Bug #7) is also
/// consumed by codegen's comparison-operand narrowing
/// (`to_llvm.rs::try_narrow_comparison`), a separate amplifier from return-type
/// narrowing. When a comparison's operand has an (incorrectly) narrow inferred
/// width, both operands are truncated to that width before comparing — so a
/// full-width value masquerading as i32 is silently truncated, giving the wrong
/// comparison result and hence wrong control flow.
///
/// `doCmp` returns `lt(sdiv(x, y), 0xffffffff)`. With `x` narrowed (callers pass
/// `and(calldataload, 0xff)`) and `y = -1`, `sdiv(5, -1) = -5 = 2^256-5`, which
/// is NOT `< 0xffffffff` (EVM result 0). But the comparison truncates the
/// quotient to i32 (`0xfffffffb`) and computes `lt(0xfffffffb, 0xffffffff) = 1`.
object "SdivCompareNarrow" {
    code {
        let size := datasize("SdivCompareNarrow_deployed")
        codecopy(0, dataoffset("SdivCompareNarrow_deployed"), size)
        return(0, size)
    }
    object "SdivCompareNarrow_deployed" {
        code {
            let r0, w0 := doCmp(and(calldataload(0), 0xff), calldataload(32))
            let r1, w1 := doCmp(and(calldataload(64), 0xff), calldataload(96))
            let r2, w2 := doCmp(and(calldataload(128), 0xff), calldataload(160))
            let r3, w3 := doCmp(and(calldataload(192), 0xff), calldataload(224))
            mstore(0x80, r0)
            mstore(0xa0, w0)
            mstore(0xc0, r1)
            mstore(0xe0, w1)
            mstore(0x100, r2)
            mstore(0x120, w2)
            mstore(0x140, r3)
            mstore(0x160, w3)
            return(0x80, 256)

            function doCmp(x, y) -> r, w {
                let s := sdiv(x, y)
                r := lt(s, 0xffffffff)
                let a1 := add(y, 1)
                let a2 := mul(a1, 3)
                let a3 := xor(a2, 7)
                let a4 := add(a3, 9)
                let a5 := or(a4, 13)
                let a6 := add(a5, 17)
                let a7 := mul(a6, 19)
                let a8 := xor(a7, 23)
                w := add(a8, 29)
            }
        }
    }
}
