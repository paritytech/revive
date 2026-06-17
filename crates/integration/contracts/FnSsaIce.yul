object "GF" {
  code { datacopy(0, dataoffset("GF_deployed"), datasize("GF_deployed")) return(0, datasize("GF_deployed")) }
  object "GF_deployed" {
    code {
      function f(p0, p1) -> r0, r1 {
        r0 := 0 r1 := 0
        if xor(4294967295, 32) { r0 := p1 leave }
        if shr(p1, p1) { r0 := p1 leave }
        if 32 { r1 := and(sub(340282366920938463463374607431768211456, 1461501637330902918203684832716283019655932542975), 0xFF) leave }
        if sar(255, p1) { r1 := and(smod(5315799647723455265, p1), 0xFF) leave }
      }
      let a := calldataload(0)
      let b := calldataload(32)
      let c := calldataload(64)
      let d := calldataload(96)
      let s0, s1 := f(a, b)
      let t0, t1 := f(c, d)
      let r := xor(xor(s0, s1), xor(t0, t1))
      mstore(0, r)
      return(0, 32)
    }
  }
}
