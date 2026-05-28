//! Compilation paths the differential harness compares.
//!
//! Each helper returns `Result<Vec<u8>, anyhow::Error>` (or `String`
//! for Yul IR) — never panics. Internal compile-paths in `resolc`
//! and `solc` use `.expect()` on degenerate inputs, so we wrap them
//! in [`catch_unwind`] and convert any panic to an `Err`.
//!
//! Uncached on purpose: `resolc::test_utils::{compile_blob, ...}`
//! memoises by `(name, source)`, which never repeats under a fuzzer
//! that emits a fresh suffix per case. The cache would just leak.

use std::collections::BTreeMap;
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::{Command, Stdio};

use resolc::test_utils::{build_solidity_with_options, build_solidity_with_options_evm};
use revive_llvm_context::OptimizerSettings;
use revive_solc_json_interface::{
    SolcStandardJsonInputSource, SolcStandardJsonOutputErrorHandler,
};
use revive_yul::lexer::Lexer;
use revive_yul::parser::statement::object::Object;
use revive_yul::visitor::{AstNode, Printer};

use crate::panic_util::panic_to_string;

const FILE_NAME: &str = "contract.sol";

fn sources(source: &str) -> BTreeMap<String, SolcStandardJsonInputSource> {
    BTreeMap::from([(
        FILE_NAME.to_owned(),
        SolcStandardJsonInputSource::from(source.to_owned()),
    )])
}

/// Direct `solc → EVM`. Used by `run_case_solc_evm` / libFuzzer.
pub fn solc_evm(contract_name: &str, source: &str) -> anyhow::Result<Vec<u8>> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        build_solidity_with_options_evm(sources(source), Default::default(), Default::default(), true)
    }))
    .map_err(|payload| anyhow::anyhow!("solc EVM compile panicked: {}", panic_to_string(payload)))?;
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
    .map_err(|payload| anyhow::anyhow!("resolc PVM compile panicked: {}", panic_to_string(payload)))?;
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

/// `solc → Yul → revive-yul reprint → solc --strict-assembly → EVM`.
/// Divergence against [`solc_evm`] points at a revive-yul printer bug.
pub fn revive_yul_roundtrip_evm(contract_name: &str, source: &str) -> anyhow::Result<Vec<u8>> {
    let yul = solc_to_yul(contract_name, source)?;
    let reprinted = reparse_and_reprint_yul(&yul)?;
    yul_to_evm_bytecode(&reprinted, contract_name)
}

/// Ask `solc` for the optimised Yul IR of `contract_name`. Invokes
/// `solc --ir-optimized` as a subprocess so failures surface as
/// `Err(...)` instead of panicking the fuzzer process.
fn solc_to_yul(contract_name: &str, source: &str) -> anyhow::Result<String> {
    let mut child = Command::new("solc")
        .args(["--ir-optimized", "--optimize", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| anyhow::anyhow!("spawn solc: {error}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("solc stdin unavailable"))?
        .write_all(source.as_bytes())
        .map_err(|error| anyhow::anyhow!("solc stdin write: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| anyhow::anyhow!("solc wait: {error}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "solc --ir-optimized failed for {contract_name}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    parse_ir_optimized(&output.stdout, contract_name)
}

/// solc emits either `Optimized IR:\n<yul>` (single-contract) or
/// per-contract blocks separated by `======= <file>:<name> =======`.
/// solc auto-suffixes the object id (`:Probe_31`), so we accept
/// `:<name>` followed by `_` or ` `.
fn parse_ir_optimized(stdout: &[u8], contract_name: &str) -> anyhow::Result<String> {
    let text = std::str::from_utf8(stdout)
        .map_err(|error| anyhow::anyhow!("solc stdout utf8: {error}"))?;

    let body_search: &str = match find_header_offset(text, contract_name) {
        Some(off) => &text[off..],
        None if has_header_lines(text) => {
            anyhow::bail!("no `:{contract_name}` header in solc output")
        }
        None => text,
    };

    let body_start = body_search
        .find("Optimized IR:")
        .ok_or_else(|| anyhow::anyhow!("no `Optimized IR:` marker for {contract_name}"))?;
    let body = &body_search[body_start + "Optimized IR:".len()..];
    let body_end = body.find("=======").unwrap_or(body.len());
    let yul = body[..body_end].trim();
    if yul.is_empty() {
        anyhow::bail!("empty Yul body for {contract_name}");
    }
    Ok(yul.to_string())
}

/// Any line starts with `======= ` (after CR trim)?
fn has_header_lines(text: &str) -> bool {
    text.split_terminator('\n')
        .any(|line| line.trim_end_matches('\r').starts_with("======= "))
}

/// Byte offset of the `======= <…>:<contract_name>{_| } =======`
/// header line in `text`, or `None`. CRLF-safe.
fn find_header_offset(text: &str, contract_name: &str) -> Option<usize> {
    let needle = format!(":{contract_name}");
    let mut cursor = 0;
    for line in text.split_terminator('\n') {
        let stripped = line.trim_end_matches('\r');
        let line_offset = cursor;
        cursor += line.len() + 1; // +1 for the consumed '\n'
        if !stripped.starts_with("======= ") {
            continue;
        }
        let Some(after) = stripped.split_once(&needle).map(|(_, after)| after) else {
            continue;
        };
        let next = after.chars().next();
        if next == Some('_') || next == Some(' ') {
            return Some(line_offset);
        }
    }
    None
}

fn reparse_and_reprint_yul(yul_source: &str) -> anyhow::Result<String> {
    let mut lexer = Lexer::new(yul_source.to_owned());
    let object = Object::parse(&mut lexer, None)
        .map_err(|error| anyhow::anyhow!("revive-yul parse: {error:?}"))?;
    let mut printer = Printer::default();
    object.accept(&mut printer);
    Ok(printer.buffer)
}

fn yul_to_evm_bytecode(yul_source: &str, contract_name: &str) -> anyhow::Result<Vec<u8>> {
    let mut child = Command::new("solc")
        .args(["--strict-assembly", "--bin", "--optimize", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| anyhow::anyhow!("spawn solc: {error}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("solc stdin unavailable"))?
        .write_all(yul_source.as_bytes())
        .map_err(|error| anyhow::anyhow!("solc stdin write: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| anyhow::anyhow!("solc wait: {error}"))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "solc --strict-assembly failed for {contract_name}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    parse_solc_strict_assembly_bin(&output.stdout, contract_name)
}

/// First `Binary representation:` block = top-level object deploy code.
fn parse_solc_strict_assembly_bin(stdout: &[u8], contract_name: &str) -> anyhow::Result<Vec<u8>> {
    let text = std::str::from_utf8(stdout)
        .map_err(|error| anyhow::anyhow!("solc stdout utf8: {error}"))?;
    let marker = "Binary representation:";
    let start = text
        .find(marker)
        .ok_or_else(|| anyhow::anyhow!("no `{marker}` in solc output for {contract_name}"))?;
    let after = &text[start + marker.len()..];
    let hex_line = after
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| anyhow::anyhow!("empty bin block for {contract_name}"))?;
    hex::decode(hex_line).map_err(|error| anyhow::anyhow!("hex decode: {error}"))
}

#[cfg(test)]
mod tests {
    //! E2E self-tests; ignored by default (need solc + LLVM).
    //! Lock the parser contracts against solc-version drift.
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
    #[ignore = "requires solc + LLVM_SYS_221_PREFIX"]
    fn solc_to_yul_returns_nonempty_ir() {
        let yul = solc_to_yul("Probe", FIXTURE).expect("solc_to_yul");
        assert!(yul.contains("object"), "expected `object` in Yul IR:\n{yul}");
        assert!(yul.contains("Probe"), "Yul IR should mention contract:\n{yul}");
        // Sanity: 200 bytes is a generous lower bound for any non-empty
        // optimised Yul module.
        assert!(yul.len() > 200, "Yul IR suspiciously short: {} bytes", yul.len());
    }

    #[test]
    #[ignore = "requires solc"]
    fn solc_evm_returns_evm_bytecode() {
        let bytes = solc_evm("Probe", FIXTURE).expect("solc_evm");
        assert!(!bytes.is_empty(), "EVM bytecode empty");
        // First instruction of any Solidity 0.8 deploy code is PUSH0 / PUSH1.
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
        assert!(bytes.len() >= 3, "PVM blob too small: {} bytes", bytes.len());
        assert_eq!(&bytes[..3], b"PVM", "PVM magic missing");
    }

    #[test]
    #[ignore = "requires solc"]
    fn revive_yul_roundtrip_evm_returns_bytecode() {
        let bytes = revive_yul_roundtrip_evm("Probe", FIXTURE).expect("roundtrip");
        assert!(!bytes.is_empty(), "roundtrip EVM bytecode empty");
        assert!(
            bytes[0] == 0x5f || bytes[0] == 0x60,
            "unexpected first opcode 0x{:02x}",
            bytes[0]
        );
    }

    // Synthetic-stdout tests for `parse_ir_optimized`; no solc.

    #[test]
    fn parse_ir_optimized_single_contract() {
        let stdout = b"Optimized IR:\nobject \"Probe_31\" {\n  code { return(0,0) }\n}\n";
        let yul = parse_ir_optimized(stdout, "Probe").unwrap();
        assert!(yul.starts_with("object"), "yul:\n{yul}");
    }

    #[test]
    fn parse_ir_optimized_multi_contract_prefix_collision() {
        // `:Foo` must not match `:FooBar` — boundary char (_| ) required.
        let stdout = b"\
======= <stdin>:FooBar_42 =======\n\
\n\
Optimized IR:\n\
object \"FooBar_42\" { code {} }\n\
\n\
======= <stdin>:Foo_99 =======\n\
\n\
Optimized IR:\n\
object \"Foo_99\" { code { sstore(0,1) } }\n\
";
        let yul = parse_ir_optimized(stdout, "Foo").unwrap();
        assert!(yul.contains("Foo_99"), "got FooBar's body: {yul}");
        assert!(!yul.contains("FooBar_42"), "got FooBar's body: {yul}");
    }

    #[test]
    fn parse_ir_optimized_no_trailing_newline() {
        // Truncated stdout must not panic.
        let stdout = b"Optimized IR:\nobject \"Probe_31\" { code {} }";
        let yul = parse_ir_optimized(stdout, "Probe").unwrap();
        assert!(yul.contains("object"));
    }

    #[test]
    fn parse_ir_optimized_multi_contract_no_trailing_newline() {
        // Multi-contract + truncated. find_header_offset must stay sound.
        let stdout = b"\
======= <stdin>:Other_1 =======\n\
\n\
Optimized IR:\n\
object \"Other_1\" { code {} }\n\
\n\
======= <stdin>:Probe_2 =======\n\
\n\
Optimized IR:\n\
object \"Probe_2\" { code { sstore(0,7) } }";
        let yul = parse_ir_optimized(stdout, "Probe").unwrap();
        assert!(yul.contains("Probe_2"), "got: {yul}");
        assert!(yul.contains("sstore(0,7)"), "got: {yul}");
    }

    #[test]
    fn parse_ir_optimized_crlf_line_endings() {
        // CRLF: `\r` stays in line.len(), only `\n` is the +1.
        let stdout = b"\
======= <stdin>:Other_1 =======\r\n\
\r\n\
Optimized IR:\r\n\
object \"Other_1\" { code {} }\r\n\
\r\n\
======= <stdin>:Probe_2 =======\r\n\
\r\n\
Optimized IR:\r\n\
object \"Probe_2\" { code { sstore(0,7) } }\r\n\
";
        let yul = parse_ir_optimized(stdout, "Probe").unwrap();
        assert!(yul.contains("Probe_2"), "got: {yul}");
    }
}
