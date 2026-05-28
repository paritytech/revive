//! Compile paths. Each helper returns `Result` — never panics:
//! resolc/solc internals use `.expect()` on degenerate inputs, so
//! we [`catch_unwind`] and convert to `Err`.
//!
//! Uncached on purpose — `resolc::test_utils::{compile_blob, ...}`
//! memoises by `(name, source)`, which never repeats under fresh
//! per-case suffixes.

use std::any::Any;
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

use resolc::test_utils::{build_solidity_with_options, build_solidity_with_options_evm};
use revive_llvm_context::OptimizerSettings;
use revive_solc_json_interface::{SolcStandardJsonInputSource, SolcStandardJsonOutputErrorHandler};

const FILE_NAME: &str = "contract.sol";

fn sources(source: &str) -> BTreeMap<String, SolcStandardJsonInputSource> {
    BTreeMap::from([(
        FILE_NAME.to_owned(),
        SolcStandardJsonInputSource::from(source.to_owned()),
    )])
}

fn panic_to_string(payload: Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// Direct `solc → EVM`. Used by `run_case_solc_evm` / libFuzzer.
pub fn solc_evm(contract_name: &str, source: &str) -> anyhow::Result<Vec<u8>> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        build_solidity_with_options_evm(
            sources(source),
            Default::default(),
            Default::default(),
            true,
        )
    }))
    .map_err(|payload| {
        anyhow::anyhow!("solc EVM compile panicked: {}", panic_to_string(payload))
    })?;
    let contracts = result.map_err(|error| anyhow::anyhow!("solc EVM compile: {error}"))?;
    let (bytecode, _runtime) = contracts
        .get(contract_name)
        .ok_or_else(|| anyhow::anyhow!("contract {contract_name} missing from solc EVM output"))?;
    hex::decode(bytecode.object.as_str())
        .map_err(|error| anyhow::anyhow!("solc EVM bytecode hex decode: {error}"))
}

/// `resolc → PVM`. Returns a PolkaVM blob.
pub fn resolc_pvm(contract_name: &str, source: &str) -> anyhow::Result<Vec<u8>> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        build_solidity_with_options(
            sources(source),
            Default::default(),
            Default::default(),
            OptimizerSettings::cycles(),
            true,
            Default::default(),
        )
    }))
    .map_err(|payload| {
        anyhow::anyhow!("resolc PVM compile panicked: {}", panic_to_string(payload))
    })?;
    let output = result.map_err(|error| anyhow::anyhow!("resolc PVM compile: {error}"))?;
    if output.has_errors() {
        anyhow::bail!("resolc PVM compile reported errors");
    }
    let bytecode = output
        .contracts
        .get(FILE_NAME)
        .and_then(|m| m.get(contract_name))
        .and_then(|c| c.evm.as_ref())
        .and_then(|e| e.bytecode.as_ref())
        .ok_or_else(|| anyhow::anyhow!("PVM bytecode missing for {contract_name}"))?;
    hex::decode(bytecode.object.as_str())
        .map_err(|error| anyhow::anyhow!("PVM bytecode hex decode: {error}"))
}

#[cfg(test)]
mod tests {
    //! E2E self-tests; ignored by default (need solc + LLVM).
    //! `cargo test -p revive-fuzz --lib -- --ignored`.
    use super::*;

    const FIXTURE: &str = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8;
contract Probe {
    uint256 public slot;
    constructor(uint256 seed) { slot = seed; }
    function fn_0(uint256 arg) external returns (uint256) {
        unchecked { slot = slot + arg; }
        return slot;
    }
}
"#;

    #[test]
    #[ignore = "requires solc"]
    fn solc_evm_returns_evm_bytecode() {
        let bytes = solc_evm("Probe", FIXTURE).expect("solc_evm");
        assert!(!bytes.is_empty(), "EVM bytecode empty");
        // Solidity 0.8 deploy code starts with PUSH0/PUSH1.
        assert!(
            bytes[0] == 0x5f || bytes[0] == 0x60,
            "unexpected first opcode 0x{:02x}",
            bytes[0]
        );
    }

    #[test]
    #[ignore = "requires solc + LLVM_SYS_221_PREFIX"]
    fn resolc_pvm_returns_pvm_blob() {
        let bytes = resolc_pvm("Probe", FIXTURE).expect("resolc_pvm");
        assert!(
            bytes.len() >= 3,
            "PVM blob too small: {} bytes",
            bytes.len()
        );
        assert_eq!(&bytes[..3], b"PVM", "PVM magic missing");
    }
}
