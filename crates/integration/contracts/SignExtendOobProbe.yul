/// Probe: `signextend(32, x)` — byte index 32 is out of range. EVM returns
/// the value unchanged (no extension). stdlib `__signextend` guards via
/// `icmp uge 31`. Tests that the guard survives O3.
object "SignExtendOobProbe" {
    code {
        let size := datasize("SignExtendOobProbe_deployed")
        codecopy(0, dataoffset("SignExtendOobProbe_deployed"), size)
        return(0, size)
    }
    object "SignExtendOobProbe_deployed" {
        code {
            let tag := calldataload(0)
            let value := calldataload(32)
            let numbyte := shl(5, 1)  // = 32, obfuscated
            mstore(0, xor(signextend(numbyte, value), tag))
            return(0, 32)
        }
    }
}
