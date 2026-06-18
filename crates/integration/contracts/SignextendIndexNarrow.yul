/// Probe: if newyork narrows the signextend BYTE-INDEX parameter to i64, a huge
/// index (>=31, which EVM treats as "leave value unchanged") truncated to a
/// small index would sign-extend from a low byte instead, changing the value.
/// `pp` is used as mload offset (case 0) to drive narrowing, and as the
/// signextend index (case 1). value 0x80 has bit 7 set, so signextend(0, 0x80)
/// = 0xff..ff80 while signextend(huge, 0x80) = 0x80.
object "SignextendIndexNarrow" {
    code { let s := datasize("SignextendIndexNarrow_deployed") codecopy(0, dataoffset("SignextendIndexNarrow_deployed"), s) return(0, s) }
    object "SignextendIndexNarrow_deployed" {
        code {
            let op := calldataload(0)
            let p := calldataload(32)
            let r0 := probe(op, p)
            let r1 := probe(op, p)
            let r2 := probe(op, p)
            let r3 := probe(op, p)
            mstore(0x200, r0) mstore(0x220, add(r1, add(r2, r3)))
            return(0x200, 64)
            function probe(opx, pp) -> r {
                switch opx
                case 0 { r := mload(pp) }
                case 1 { r := signextend(pp, 0x80) }
                case 2 { r := keccak256(pp, 32) }
                default { r := pp }
            }
        }
    }
}
