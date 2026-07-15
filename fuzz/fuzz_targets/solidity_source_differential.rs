//! libFuzzer entry: source-text Solidity differential.
//!
//! Bytes are interpreted as UTF-8 Solidity source (e.g. LLM-generated
//! contracts seeded into the corpus via `make llm-corpus`), compiled by
//! `solc → EVM` and `resolc → PVM`, then executed against a fixed calling
//! convention (`constructor(uint256 seed)` + repeated `fn_0(uint256)`).
//! Divergence panics; libFuzzer writes the crashing `.sol` bytes to
//! `fuzz/artifacts/solidity_source_differential/`.
//!
//! Unlike `solidity_differential` (fuzzer bytes → `arbitrary` decision tape),
//! here the bytes ARE the contract, so byte-level mutation edits Solidity text
//! directly. Pair with `-dict=solidity.dict` for productive text mutation.

#![no_main]

use libfuzzer_sys::fuzz_target;
use revive_fuzz::panic_on_divergence::run_source_case_panic;

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    run_source_case_panic(source);
});
