object "TstoreProbe" {
  code { datacopy(0, dataoffset("TstoreProbe_deployed"), datasize("TstoreProbe_deployed")) return(0, datasize("TstoreProbe_deployed")) }
  object "TstoreProbe_deployed" {
    code {
      let a := calldataload(0)
      let b := calldataload(32)
      let r := 0
      tstore(a, b)
      tstore(add(a, 1), not(b))
      r := xor(r, tload(a))
      r := xor(r, tload(add(a, 1)))
      r := xor(r, tload(add(a, 2)))
      tstore(a, and(b, 0xFF))
      r := add(r, tload(a))
      mstore(0, r)
      return(0, 32)
    }
  }
}
