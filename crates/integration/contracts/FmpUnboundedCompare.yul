/// Soundness PoC: newyork forward inference gives a `FreePointerSlot` `mload`
/// (the free-memory pointer at 0x40) the width I32, sound only while the FMP
/// holds a Solidity-allocator pointer (`< heap_size`). When a write to 0x40 is
/// not provably sbrk-bounded, `heap_opt` sets `fmp_could_be_unbounded` and
/// codegen loads the *full* FMP word (no `FMP < heap_size` range proof) — but
/// inference still said I32, so `gt(v, 0xffffffff)` was evaluated at 32 bits and
/// truncated the live value.
///
/// `mstore(0x40, calldataload(0))` taints the FMP with an arbitrary value; the
/// `staticcall` is a memory barrier so the following `mload(0x40)` is a real load
/// rather than store-forwarded. With `v = 2^40` (> 2^32) the comparison must be
/// `1`; the bug truncates `v` to its low 32 bits (`0`) and yields `0`.
object "FmpUnboundedCompare" {
    code {
        let size := datasize("FmpUnboundedCompare_deployed")
        codecopy(0, dataoffset("FmpUnboundedCompare_deployed"), size)
        return(0, size)
    }
    object "FmpUnboundedCompare_deployed" {
        code {
            let taint := calldataload(0)
            mstore(0x40, taint)
            let ok := staticcall(gas(), 0, 0, 0, 0, 0)
            let v := mload(0x40)
            let out := 0
            if gt(v, 0xffffffff) {
                out := 1
            }
            mstore(0, out)
            return(0, 32)
        }
    }
}
