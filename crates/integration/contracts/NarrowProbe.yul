object "NarrowProbe" {
    code {
        datacopy(0, dataoffset("NarrowProbe_deployed"), datasize("NarrowProbe_deployed"))
        return(0, datasize("NarrowProbe_deployed"))
    }
    object "NarrowProbe_deployed" {
        code {
            let op := calldataload(0)
            let a := calldataload(32)
            let r := 0
            switch op
            // high-bit round trip: narrow via shr then widen back via shl
            case 0 { r := shl(200, shr(200, a)) }
            // narrow to i32 then square (needs up to 64 bits)
            case 1 { let t := and(a, 0xFFFFFFFF) r := mul(t, t) }
            // narrow to i64 then shift into the high half
            case 2 { let t := shr(192, a) r := shl(192, t) }
            // narrow store then load round trip (i16)
            case 3 { let t := and(a, 0xFFFF) mstore(0, t) r := mload(0) }
            // top byte (i8) shifted back to the top
            case 4 { let t := byte(0, a) r := shl(248, t) }
            // narrow to i8 then storage round trip
            case 5 { let t := and(a, 0xFF) sstore(5, t) r := sload(5) }
            // narrow then negate: 0 - small => full-width large value
            case 6 { let t := shr(128, a) r := sub(0, t) }
            // not of a narrow value => high bits all set (full width)
            case 7 { let t := and(a, 0xFFFFFFFF) r := not(t) }
            // i64 + i64 => up to 65 bits
            case 8 { let t := and(a, 0xFFFFFFFFFFFFFFFF) r := add(t, t) }
            // signextend over a narrowed value
            case 9 { let t := shr(64, a) r := signextend(7, t) }
            // narrow then compare-against-large: lt(small, big) must be 1
            case 10 { let t := and(a, 0xFF) r := lt(t, 0x100000000) }
            // div of a narrowed dividend by 1 then widen-add a big constant
            case 11 { let t := and(a, 0xFFFF) r := add(div(t, 1), shl(200, 1)) }
            // mulmod where two operands are narrow but the product overflows 256
            case 12 { let t := and(a, 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF) r := mulmod(t, t, 0x100000000000000000000000000000000) }
            // narrow value used as exp base, large exponent
            case 13 { let t := and(a, 0xFF) r := exp(t, 64) }
            mstore(0, r)
            return(0, 32)
        }
    }
}
