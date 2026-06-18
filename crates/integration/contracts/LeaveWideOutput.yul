/// Soundness PoC: newyork forward type inference widens an `if`/`switch`
/// output only from the then/else region *yields*, never from `inputs`. But on
/// a missing `else`/`default` edge codegen routes `inputs` straight through to
/// `outputs` (`to_llvm` phi construction; the IR `If` "defaults to yielding
/// inputs unchanged" contract). The `leave`-elimination wrapper
/// (`inline.rs::wrap_remaining_in_guard`) builds exactly that shape:
/// `else_region: None`, non-empty `outputs`, where `inputs` are the
/// pre-`leave` accumulators and the then-region yields the fall-through values.
///
/// `f(v)` sets `ret := v` (full width), takes `leave` for `v > 1000` (carrying
/// the wide `ret`), and otherwise falls through to `ret := 7` (narrow). Inference
/// widened the output only from the `7` yield → i8, so codegen truncates the
/// leave-edge value to its low byte. `from_yul` always emits an explicit else,
/// so only the post-inlining `leave` shape is affected.
///
/// Called with `v = 2^200` (> 1000): EVM returns `2^200`; the bug returns
/// `2^200 mod 256 == 0`.
object "LeaveWideOutput" {
    code {
        let size := datasize("LeaveWideOutput_deployed")
        codecopy(0, dataoffset("LeaveWideOutput_deployed"), size)
        return(0, size)
    }
    object "LeaveWideOutput_deployed" {
        code {
            let x := calldataload(0)
            let r := f(x)
            let out := 0
            if gt(r, 5) {
                out := 1
            }
            mstore(0, out)
            return(0, 32)

            function f(v) -> ret {
                ret := v
                if gt(v, 1000) {
                    leave
                }
                ret := 7
            }
        }
    }
}
