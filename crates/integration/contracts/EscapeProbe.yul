object "EscapeProbe" {
    code {
        datacopy(0, dataoffset("EscapeProbe_deployed"), datasize("EscapeProbe_deployed"))
        return(0, datasize("EscapeProbe_deployed"))
    }
    object "EscapeProbe_deployed" {
        code {
            let op := calldataload(0)
            let a := calldataload(32)
            switch op
            case 0 { mstore(0, a) log0(0, 32) return(0, 0) }
            case 1 { mstore(0, a) log1(0, 32, 0x42) return(0, 0) }
            case 2 { mstore(0x40, a) log0(0x40, 32) return(0, 0) }
            case 3 { let p := mload(0x40) mstore(p, a) log0(p, 64) return(0, 0) }
            case 4 { mstore(0, a) mstore(32, not(a)) log0(0, 64) return(0, 0) }
            case 5 { mstore(0, a) revert(0, 32) }
            case 6 { mstore(0x40, a) mstore8(0x60, 0xCD) log0(0x40, 33) return(0, 0) }
            case 7 { mstore(0, a) return(0, 32) }
            case 8 { mstore(0x40, a) return(0x40, 32) }
            case 9 { mstore(0, a) mstore8(7, 0x99) revert(0, 32) }
            case 10 { mstore(0, a) log2(0, 32, a, 0x11) return(0, 0) }
            case 11 { let p := mload(0x40) mstore(p, a) mstore(add(p, 32), a) return(p, 64) }
            default { return(0, 0) }
        }
    }
}
