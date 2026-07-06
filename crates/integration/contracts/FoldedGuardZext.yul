/// ICE PoC (paritytech/revive#560): the one-armed `if` lowering in `to_llvm`
/// built the conditional branch first and materialized the fall-through
/// `inputs` afterwards, with the builder still positioned at the end of the
/// now-terminated entry block. That is invisible while every input is already
/// word-typed (no instruction is emitted), but a narrow input — here the raw
/// `gt` comparison result flowing out of the inlined `fun_h0` — needs a `zext`
/// to reach the join phi, and that `zext` landed after the terminator:
/// "Basic Block in function 'fun_run_runtime' does not have terminator!".
///
/// The shape needs all of:
///   - `fun_h0`'s `if 0x01 { ... leave }`: the constant-true guard folds, so
///     the `leave`-carried return value copy-propagates to the bare `gt`
///     (an i1 value that is neither a constant nor a word-typed if-output);
///   - `fun_h1(acc, acc)`: equal arguments fold `iszero(lt(a, b))` to
///     constant true, collapsing the leave-`if` so the `leave`-elimination
///     guard around the remainder keeps the raw `gt` as its fall-through
///     input with a constant-false condition;
///   - the rotated `for` loop (condition `1`, `break` in the body — solc's
///     unoptimized lowering) before that guard, which keeps the simplifier
///     from folding the constant-condition `if` away;
///   - `fun_run` staying above the never-inline size threshold (the verbose
///     copy chains are load-bearing): fully inlined into the entry block,
///     the guard folds and the shape disappears.
///
/// This is solc's `--no-optimize-yul` output for issue #560's contract with
/// the selector dispatch and revert-helper indirections trimmed. `run(s)`
/// never assigns its unnamed return value, so every call must return 0.
object "FoldedGuardZext" {
    code {
        let size := datasize("FoldedGuardZext_deployed")
        codecopy(0, dataoffset("FoldedGuardZext_deployed"), size)
        return(0, size)
    }
    object "FoldedGuardZext_deployed" {
        code {
            {
                mstore(64, memoryguard(0x80))
                external_fun_run()
                revert(0, 0)
            }
            function allocate_unbounded() -> memPtr
            { memPtr := mload(64) }
            function cleanup_uint256(value) -> cleaned
            { cleaned := value }
            function validator_revert_uint256(value)
            {
                if iszero(eq(value, cleanup_uint256(value))) { revert(0, 0) }
            }
            function abi_decode_uint256(offset, end) -> value
            {
                value := calldataload(offset)
                validator_revert_uint256(value)
            }
            function abi_decode_tuple_uint256(headStart, dataEnd) -> value0
            {
                if slt(sub(dataEnd, headStart), 32)
                {
                    revert(0, 0)
                }
                let offset := 0
                value0 := abi_decode_uint256(add(headStart, offset), dataEnd)
            }
            function abi_encode_uint256_to_uint256(value, pos)
            {
                mstore(pos, cleanup_uint256(value))
            }
            function abi_encode_uint256(headStart, value0) -> tail
            {
                tail := add(headStart, 32)
                abi_encode_uint256_to_uint256(value0, add(headStart, 0))
            }
            function external_fun_run()
            {
                if callvalue()
                {
                    revert(0, 0)
                }
                let param := abi_decode_tuple_uint256(4, calldatasize())
                let ret := fun_run(param)
                let memPos := allocate_unbounded()
                let memEnd := abi_encode_uint256(memPos, ret)
                return(memPos, sub(memEnd, memPos))
            }
            function zero_value_for_split_uint256() -> ret
            { ret := 0 }
            function fun_run(var_s) -> var
            {
                let zero_uint256 := zero_value_for_split_uint256()
                var := zero_uint256
                let _1 := var_s
                let expr := _1
                let var_acc := expr
                let _2 := var_acc
                let expr_1 := _2
                let _3 := var_acc
                let expr_2 := _3
                let expr_3 := fun_h0(expr_1, expr_2)
                var_acc := expr_3
                let _4 := var_acc
                let expr_4 := _4
                let _5 := var_acc
                let expr_5 := _5
                let expr_6 := fun_h1(expr_4, expr_5)
                var_acc := expr_6
            }
            function fun_h0(var_a, var_b) -> var_r
            {
                let zero_t_uint256 := zero_value_for_split_uint256()
                var_r := zero_t_uint256
                let expr := 0x01
                if expr
                {
                    var_r := gt(var_r, var_b)
                    let _1 := var_r
                    let expr_1 := _1
                    var_r := expr_1
                    leave
                }
            }
            function cleanup_t_rational_by(value) -> cleaned
            { cleaned := value }
            function identity(value) -> ret
            { ret := value }
            function convert_t_rational_by_to_t_uint256(value) -> converted
            {
                converted := cleanup_uint256(identity(cleanup_t_rational_by(value)))
            }
            function increment_wrapping_uint256(value) -> ret
            {
                ret := cleanup_uint256(add(value, 1))
            }
            function cleanup_rational_by(value) -> cleaned
            { cleaned := value }
            function convert_rational_by_to_uint256(value) -> converted
            {
                converted := cleanup_uint256(identity(cleanup_rational_by(value)))
            }
            function fun_h1(var_a, var_b) -> var_r
            {
                let zero_uint256 := zero_value_for_split_uint256()
                var_r := zero_uint256
                let expr := 0x00
                let var_i := convert_t_rational_by_to_t_uint256(expr)
                for { }
                1
                {
                    let _1 := var_i
                    let _2 := increment_wrapping_uint256(_1)
                    var_i := _2
                }
                {
                    let _3 := var_i
                    let expr_1 := _3
                    let expr_2 := 0x01
                    let expr_3 := lt(cleanup_uint256(expr_1), convert_rational_by_to_uint256(expr_2))
                    if iszero(expr_3) { break }
                }
                let _4 := var_a
                let expr_4 := _4
                let _5 := var_b
                let expr_5 := _5
                let expr_6 := iszero(lt(cleanup_uint256(expr_4), cleanup_uint256(expr_5)))
                if expr_6
                {
                    let _6 := var_a
                    let expr_7 := _6
                    var_r := expr_7
                    leave
                }
            }
        }
    }
}
