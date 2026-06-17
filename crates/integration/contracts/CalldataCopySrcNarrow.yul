/// Soundness PoC (newyork-specific): the `offx` parameter of `probe` is used as
/// an mload offset (case 0) which makes newyork narrow it to i64, and as a
/// `calldatacopy` SOURCE offset (case 1). newyork's CallDataCopy lowering treats
/// the calldata source offset as a heap pointer (`narrow_offset_for_pointer`),
/// so a large source offset traps on the PVM heap-bounds check / mis-sized copy
/// instead of zero-filling like EVM. 4 call sites keep `probe` non-inlined.
/// Reproduces only under RESOLC_USE_NEWYORK=1 (the Yul path is correct).
object "CalldataCopySrcNarrow" {
    code { let s := datasize("CalldataCopySrcNarrow_deployed") codecopy(0, dataoffset("CalldataCopySrcNarrow_deployed"), s) return(0, s) }
    object "CalldataCopySrcNarrow_deployed" {
        code {
            let op := calldataload(0)
            let off := calldataload(32)
            let r0 := probe(op, off)
            let r1 := probe(op, off)
            let r2 := probe(op, off)
            let r3 := probe(op, off)
            mstore(0x200, r0)
            mstore(0x220, add(r1, add(r2, r3)))
            return(0x200, 64)
            function probe(opx, offx) -> r {
                switch opx
                case 0 { r := mload(offx) }
                case 1 { calldatacopy(0x80, offx, 32) r := mload(0x80) }
                case 2 { r := keccak256(offx, 32) }
                default { r := offx }
            }
        }
    }
}
