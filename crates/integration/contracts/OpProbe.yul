object "OpProbe" {
    code {
        datacopy(0, dataoffset("OpProbe_deployed"), datasize("OpProbe_deployed"))
        return(0, datasize("OpProbe_deployed"))
    }
    object "OpProbe_deployed" {
        code {
            let op := calldataload(0)
            let a := calldataload(32)
            let b := calldataload(64)
            let c := calldataload(96)
            let r := 0
            switch op
            case 0 { r := shl(a, b) }
            case 1 { r := shr(a, b) }
            case 2 { r := sar(a, b) }
            case 3 { r := byte(a, b) }
            case 4 { r := signextend(a, b) }
            case 5 { r := sdiv(a, b) }
            case 6 { r := smod(a, b) }
            case 7 { r := exp(a, b) }
            case 8 { r := addmod(a, b, c) }
            case 9 { r := mulmod(a, b, c) }
            case 10 { r := div(a, b) }
            case 11 { r := mod(a, b) }
            case 12 { r := slt(a, b) }
            case 13 { r := sgt(a, b) }
            case 14 { r := lt(a, b) }
            case 15 { r := gt(a, b) }
            case 16 { r := not(a) }
            case 17 { r := addmod(mul(a, b), c, a) }
            mstore(0, r)
            return(0, 32)
        }
    }
}
