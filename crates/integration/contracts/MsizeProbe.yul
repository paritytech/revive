/// Probe: msize after various memory-touching ops. EVM msize = highest accessed
/// byte rounded up to a word. newyork tracks msize via ensure_heap_size; any
/// divergence corrupts Solidity's allocator (which reads msize/FMP).
object "MsizeProbe" {
    code { let s := datasize("MsizeProbe_deployed") codecopy(0, dataoffset("MsizeProbe_deployed"), s) return(0, s) }
    object "MsizeProbe_deployed" {
        code {
            let op := calldataload(0)
            let x := calldataload(32)
            let r := 0
            switch op
            case 0 { mstore(0x80, x) r := msize() }
            case 1 { mstore8(0x95, x) r := msize() }
            case 2 { let _v := mload(0x100) r := msize() }
            case 3 { mstore(0x80, x) calldatacopy(0x200, 0, 16) r := msize() }
            case 4 { r := msize() }
            case 5 { mstore(0x80, x) let _v := mload(0x81) r := msize() }
            case 6 { mstore8(0x100, x) r := msize() }
            case 7 { mstore(and(x, 0xfe0), x) r := msize() }
            default { r := 0 }
            mstore(0x400, r)
            return(0x400, 32)
        }
    }
}
