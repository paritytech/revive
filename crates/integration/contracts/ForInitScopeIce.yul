/// Soundness/robustness PoC: newyork's SSA builder (`ssa.rs`) does not place a
/// variable declared in a `for`-loop INIT block into the scope visible to the
/// loop BODY and POST blocks. In Yul, the init-block declarations are scoped to
/// the entire `for` statement (condition, body, post). The canonical Yul loop
///
///     for { let j := 0 } lt(j, a) { j := add(j, 1) } { ... use j ... }
///
/// therefore panics with `ICE: SsaBuilder::assign called for undeclared
/// variable 'j'` when the body/post references `j`.
///
/// solc's ForLoopInitRewriter hoists init declarations out of the init block
/// (emitting an empty init), so the Solidity frontend — even inline assembly —
/// never produces this shape. Direct Yul input (`resolc --yul` / `Code::Yul`)
/// does, and fails to compile. Fail-safe (ICE, not a silent miscompile), but a
/// valid program the newyork pipeline cannot compile.
object "ForInitScopeIce" {
    code {
        let size := datasize("ForInitScopeIce_deployed")
        codecopy(0, dataoffset("ForInitScopeIce_deployed"), size)
        return(0, size)
    }
    object "ForInitScopeIce_deployed" {
        code {
            let a := calldataload(0)
            let sum := 0
            for { let j := 0 } lt(j, a) { j := add(j, 1) } {
                sum := add(sum, j)
            }
            mstore(0x80, sum)
            return(0x80, 32)
        }
    }
}
