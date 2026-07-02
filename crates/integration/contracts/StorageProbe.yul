object "StorageProbe" {
    code {
        datacopy(0, dataoffset("StorageProbe_deployed"), datasize("StorageProbe_deployed"))
        return(0, datasize("StorageProbe_deployed"))
    }
    object "StorageProbe_deployed" {
        code {
            let k := calldataload(0)
            let v := calldataload(32)

            // >= 9 mapping-style sstores in the `let h := keccak256(..); sstore(h, _)`
            // form so compound_outlining's keccak256_pair + sstore -> mapping_sstore
            // fusion (T9, total mapping ops >= 9) triggers. Distinct base slots so
            // the slots do not collide. The differential runner compares the full
            // storage state between newyork-PVM and solc-EVM.
            mstore(0, k) mstore(0x20, 0) let s0 := keccak256(0, 64) sstore(s0, v)
            mstore(0, k) mstore(0x20, 1) let s1 := keccak256(0, 64) sstore(s1, add(v, 1))
            mstore(0, k) mstore(0x20, 2) let s2 := keccak256(0, 64) sstore(s2, not(v))
            mstore(0, k) mstore(0x20, 3) let s3 := keccak256(0, 64) sstore(s3, shl(200, v))
            mstore(0, k) mstore(0x20, 4) let s4 := keccak256(0, 64) sstore(s4, and(v, 0xFF))
            mstore(0, add(k, 1)) mstore(0x20, 0) let s5 := keccak256(0, 64) sstore(s5, v)
            mstore(0, add(k, 2)) mstore(0x20, 0) let s6 := keccak256(0, 64) sstore(s6, sub(0, v))
            mstore(0, not(k)) mstore(0x20, 7) let s7 := keccak256(0, 64) sstore(s7, v)
            mstore(0, k) mstore(0x20, 8) let s8 := keccak256(0, 64) sstore(s8, v)
            mstore(0, k) mstore(0x20, 9) let s9 := keccak256(0, 64) sstore(s9, exp(v, 3))

            // also read one back through the fused mapping_sload path and store it plainly
            mstore(0, k) mstore(0x20, 0) let r := sload(keccak256(0, 64))
            sstore(0xFFFF, r)

            return(0, 0)
        }
    }
}
