//! Yul → PVM via [`yul_to_pvm`] (resolc's Yul-input path) and
//! Yul → EVM via [`yul_to_evm`] (`solc --strict-assembly`). Both
//! consume identical source text, so any divergence is purely
//! backend-side.

use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::{Command, Stdio};

use resolc::test_utils::build_yul;

use crate::panic_util::panic_to_string;

/// `build_yul` keys results by `"<path>:<object_identifier>"`. The
/// `catch_unwind` is essential — `build_yul`'s tail expression
/// `result.unwrap()` panics on internal compile failures that
/// `check_errors()` doesn't surface.
pub fn yul_to_pvm(contract_name: &str, source: &str) -> anyhow::Result<Vec<u8>> {
    let path = "fuzz.yul";
    let result = catch_unwind(AssertUnwindSafe(|| build_yul(&[(path, source)])))
        .map_err(|payload| anyhow::anyhow!("resolc Yul compile panicked: {}", panic_to_string(payload)))?;
    let mut blobs = result.map_err(|error| anyhow::anyhow!("resolc Yul compile: {error}"))?;
    let key = format!("{path}:{contract_name}");
    blobs
        .remove(&key)
        .ok_or_else(|| anyhow::anyhow!("resolc produced no blob for `{key}`"))
}

pub fn yul_to_evm(contract_name: &str, source: &str) -> anyhow::Result<Vec<u8>> {
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
        .write_all(source.as_bytes())
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

    parse_first_binary(&output.stdout)
}

/// First `Binary representation:` block = top-level object deploy
/// code. Subsequent blocks are nested objects (e.g. `X_deployed`).
fn parse_first_binary(stdout: &[u8]) -> anyhow::Result<Vec<u8>> {
    let text = std::str::from_utf8(stdout)
        .map_err(|error| anyhow::anyhow!("solc stdout utf8: {error}"))?;
    let marker = "Binary representation:";
    let start = text
        .find(marker)
        .ok_or_else(|| anyhow::anyhow!("no `{marker}` in solc output"))?;
    let after = &text[start + marker.len()..];
    let hex_line = after
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| anyhow::anyhow!("empty bin block after `{marker}`"))?;
    hex::decode(hex_line).map_err(|error| anyhow::anyhow!("hex decode: {error}"))
}

#[cfg(test)]
mod tests {
    //! E2E self-tests; ignored by default (need solc + LLVM).
    //! `cargo test -p revive-fuzz --lib -- --ignored`.
    use super::*;

    const FIXTURE: &str = r#"object "Probe" {
    code {
        sstore(0, 42)
        let _size := datasize("Probe_deployed")
        let _off := dataoffset("Probe_deployed")
        datacopy(0, _off, _size)
        return(0, _size)
    }
    object "Probe_deployed" {
        code {
            mstore(0, sload(0))
            return(0, 32)
        }
    }
}"#;

    #[test]
    #[ignore = "requires solc"]
    fn yul_to_evm_returns_bytecode() {
        let bytes = yul_to_evm("Probe", FIXTURE).expect("yul_to_evm");
        assert!(!bytes.is_empty(), "EVM bytecode empty");
        // First instruction of `--strict-assembly --optimize` output
        // is typically PUSH0 / PUSH1 / JUMPDEST.
        assert!(
            bytes[0] == 0x5f || bytes[0] == 0x60 || bytes[0] == 0x5b,
            "unexpected first opcode 0x{:02x}",
            bytes[0]
        );
    }

    #[test]
    #[ignore = "requires LLVM_SYS_221_PREFIX"]
    fn yul_to_pvm_returns_pvm_blob() {
        let bytes = yul_to_pvm("Probe", FIXTURE).expect("yul_to_pvm");
        assert!(bytes.len() >= 3, "PVM blob too small: {} bytes", bytes.len());
        assert_eq!(&bytes[..3], b"PVM", "PVM magic missing");
    }

    #[test]
    fn parse_first_binary_extracts_hex() {
        // Synthetic — no solc needed. Locks the parser shape.
        let stdout = b"\
======= <stdin>:Probe =======\n\
Binary representation:\n\
6080604052348015\n\
\n\
======= <stdin>:Probe_deployed =======\n\
Binary representation:\n\
deadbeef\n\
";
        let bytes = parse_first_binary(stdout).unwrap();
        // First block, not the deployed one.
        assert_eq!(bytes, vec![0x60, 0x80, 0x60, 0x40, 0x52, 0x34, 0x80, 0x15]);
    }
}
