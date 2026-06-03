object "MemProbe" {
    code {
        datacopy(0, dataoffset("MemProbe_deployed"), datasize("MemProbe_deployed"))
        return(0, datasize("MemProbe_deployed"))
    }
    object "MemProbe_deployed" {
        code {
            let op := calldataload(0)
            let a := calldataload(32)
            let r := 0
            switch op
            case 0 { mstore(0, a) r := keccak256(0, 32) }
            case 1 { mstore(0, a) mstore(32, a) r := keccak256(0, 64) }
            case 2 { mstore(0, a) r := mload(0) }
            case 3 { mstore(0, a) mstore(16, a) r := mload(0) }
            case 4 { mstore8(0, a) r := mload(0) }
            case 5 { mstore(0, a) r := keccak256(0, 1) }
            case 6 { mstore(0, a) calldatacopy(32, calldatasize(), 32) r := keccak256(0, 64) }
            case 7 { mstore(0, a) mcopy(32, 0, 32) r := keccak256(32, 32) }
            case 8 { mstore(0, a) r := msize() }
            case 9 { mstore(0x80, a) r := msize() }
            case 10 { mstore8(100, 0xAB) r := msize() }
            case 11 { mstore(0, a) mstore(32, a) mcopy(8, 0, 40) r := keccak256(0, 72) }
            case 12 { mstore(0, a) r := keccak256(0, 0) }
            case 13 { let p := mload(0x40) mstore(p, a) r := keccak256(p, 32) }
            case 14 { mstore(0, a) r := keccak256(3, 17) }
            case 15 { mstore(0, a) mstore8(5, 0xEE) r := mload(0) }
            case 16 { mstore(0, a) r := keccak256(31, 1) }
            case 17 { mstore(10, a) r := mload(0) }
            mstore(0, r)
            return(0, 32)
        }
    }
}
