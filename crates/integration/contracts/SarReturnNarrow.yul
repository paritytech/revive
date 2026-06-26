/// Soundness PoC: newyork forward-width rule for `sar` with a constant shift
/// (`type_inference.rs::infer_expression_width`, the `Shr | Sar` arm):
///
///   if let Some(shift) = known_constants[lhs] {
///       ... else { BitWidth::from_bits((256 - shift).max(1)) }
///   } else { rhs_width }
///
/// For logical `shr` this is correct (the shifted-in high bits are zero, so the
/// result fits in `256 - shift` bits). For ARITHMETIC `sar` it is NOT: when the
/// shifted value is negative, `sar` sign-extends, so the high bits stay set and
/// the result is full-width (~2^256). The rule ignores the operand's sign and
/// caps the width at `256 - shift`. `sar` also never marks the value
/// `is_signed`, so return-type narrowing then truncates the sign bits.
///
/// `doSar` returns `sar(250, value)`. For `value = -1` (2^256-1) the result is
/// `-1` (all ones) on EVM, but the width rule reports `from_bits(6) = I8`
/// (clamped to I32), so the narrowed return drops everything above 32 bits and
/// yields `0xffffffff`.
object "SarReturnNarrow" {
    code {
        let size := datasize("SarReturnNarrow_deployed")
        codecopy(0, dataoffset("SarReturnNarrow_deployed"), size)
        return(0, size)
    }
    object "SarReturnNarrow_deployed" {
        code {
            let z0, w0 := doSar(calldataload(0), calldataload(32))
            let z1, w1 := doSar(calldataload(64), calldataload(96))
            let z2, w2 := doSar(calldataload(128), calldataload(160))
            let z3, w3 := doSar(calldataload(192), calldataload(224))
            mstore(0, z0)
            mstore(32, w0)
            mstore(64, z1)
            mstore(96, w1)
            mstore(128, z2)
            mstore(160, w2)
            mstore(192, z3)
            mstore(224, w3)
            return(0, 256)

            function doSar(value, y) -> z, w {
                z := sar(250, value)
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
