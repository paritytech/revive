/// Soundness PoC: newyork's fuzzy function deduplication
/// (`simplify.rs::deduplicate_functions_fuzzy`) abstracts `switch` case
/// match values into parameterizable literal positions
/// (`fuzzy_encode_statement` emits a placeholder per case value), so two
/// functions that are structurally identical *except for their switch case
/// match values* hash to the same fuzzy form and are merged.
///
/// However `replace_literals_with_params` advances the literal counter for
/// each switch case but NEVER substitutes `case.value`. The merged canonical
/// function therefore keeps ITS OWN case values; the removed function's call
/// sites are redirected to the canonical function with the (now ignored)
/// differing values appended as dead extra arguments.
///
/// Result: `g`'s callers silently execute `f`'s switch dispatch. `g(111)`
/// should hit `g`'s `case 111` and return 1111, but after the merge it falls
/// through to the default of `f` (whose cases are 100/200/300) and returns
/// the default value. EVM (and unoptimized semantics) returns 1111.
object "FuzzySwitchBug" {
    code {
        let size := datasize("FuzzySwitchBug_deployed")
        codecopy(0, dataoffset("FuzzySwitchBug_deployed"), size)
        return(0, size)
    }
    object "FuzzySwitchBug_deployed" {
        code {
            let sel := calldataload(0)
            let x := calldataload(32)
            let r := 0
            switch sel
            case 0 { r := f(x) }
            case 1 { r := f(x) }
            case 2 { r := g(x) }
            case 3 { r := g(x) }
            default { r := 0 }
            mstore(0, r)
            return(0, 32)

            function f(a) -> out {
                let t := add(a, 1)
                t := mul(t, 2)
                t := add(t, 3)
                t := xor(t, 4)
                t := add(t, 5)
                out := 0
                switch a
                case 100 { out := add(t, 1000) }
                case 200 { out := add(t, 2000) }
                case 300 { out := add(t, 3000) }
                default  { out := 9999 }
            }
            function g(a) -> out {
                let t := add(a, 1)
                t := mul(t, 2)
                t := add(t, 3)
                t := xor(t, 4)
                t := add(t, 5)
                out := 0
                switch a
                case 111 { out := add(t, 1000) }
                case 222 { out := add(t, 2000) }
                case 333 { out := add(t, 3000) }
                default  { out := 9999 }
            }
        }
    }
}
