//! Solidity contract templates for the differential fuzzer.
//!
//! Each template renders a complete `pragma solidity ^0.8` contract
//! with the shape the harness expects:
//!
//! * `constructor(uint256 seed)` taking one 32-byte word.
//! * `function fn_0(uint256 arg) external returns (...)` taking one
//!   32-byte word per action.
//!
//! Both the constructor seed and `fn_0` arg are 32 bytes of arbitrary
//! input — templates re-interpret the bits as `int256`, opcode
//! selectors, loop bounds, …, however they like.
//!
//! All templates must compile under solc 0.8.x without warnings, and
//! must terminate deterministically on every 256-bit input pair.
//! Anything that depends on block context (`block.number`, `gas`,
//! `block.coinbase`, …) is forbidden — those differ between geth's
//! `evm` and pallet-revive's sim and would surface as benign
//! divergences.

use std::fmt::Write;

use arbitrary::{Arbitrary, Unstructured};

/// Which template family to generate. The variant chosen is mixed
/// into the contract name so a divergence report points at the right
/// template.
#[derive(Debug, Clone, Copy)]
pub enum TemplateKind {
    /// `slot0 % arg` — the original SREM probe (paritytech/revive#527).
    Srem,
    /// Two storage slots, three-op arithmetic chain.
    ArithChain,
    /// `unchecked { }`-wrapped uint256 arithmetic.
    UncheckedArith,
    /// `mapping(uint256 => uint256)` increment.
    Mapping,
    /// Dynamic `uint256[]` push + index.
    DynArray,
    /// `require(predicate, "msg")` guard.
    RequireGuard,
    /// Bounded `for` accumulator.
    LoopAccum,
    /// Pure-bitwise composition (shifts + masks).
    Bitwise,
}

impl TemplateKind {
    const ALL: &'static [TemplateKind] = &[
        TemplateKind::Srem,
        TemplateKind::ArithChain,
        TemplateKind::UncheckedArith,
        TemplateKind::Mapping,
        TemplateKind::DynArray,
        TemplateKind::RequireGuard,
        TemplateKind::LoopAccum,
        TemplateKind::Bitwise,
    ];

    fn name_prefix(self) -> &'static str {
        match self {
            TemplateKind::Srem => "Srem",
            TemplateKind::ArithChain => "Arith",
            TemplateKind::UncheckedArith => "UArith",
            TemplateKind::Mapping => "Mapping",
            TemplateKind::DynArray => "DynArray",
            TemplateKind::RequireGuard => "Require",
            TemplateKind::LoopAccum => "Loop",
            TemplateKind::Bitwise => "Bitwise",
        }
    }
}

impl<'a> Arbitrary<'a> for TemplateKind {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(*u.choose(TemplateKind::ALL)?)
    }
}

/// A rendered contract: source plus the name solc will produce.
pub struct RenderedContract {
    pub name: String,
    pub source: String,
}

/// Render a contract for the given template, drawing any sub-choices
/// (op selectors, etc.) from `u`. The contract name embeds a random
/// suffix so multiple cases in one run don't collide in the
/// blob-cache lookup.
pub fn render(kind: TemplateKind, u: &mut Unstructured<'_>) -> arbitrary::Result<RenderedContract> {
    let suffix: u32 = u.arbitrary()?;
    let name = format!("Fuzz{}_{:08x}", kind.name_prefix(), suffix);
    let source = match kind {
        TemplateKind::Srem => render_srem(&name),
        TemplateKind::ArithChain => render_arith_chain(u, &name)?,
        TemplateKind::UncheckedArith => render_unchecked_arith(u, &name)?,
        TemplateKind::Mapping => render_mapping(&name),
        TemplateKind::DynArray => render_dyn_array(&name),
        TemplateKind::RequireGuard => render_require_guard(u, &name)?,
        TemplateKind::LoopAccum => render_loop_accum(u, &name)?,
        TemplateKind::Bitwise => render_bitwise(u, &name)?,
    };
    Ok(RenderedContract { name, source })
}

// ── Op menus ───────────────────────────────────────────────────────────

const SIGNED_BIN_OPS: &[&str] = &["+", "-", "*", "/", "%"];
const UNSIGNED_BIN_OPS: &[&str] = &["+", "-", "*", "/", "%"];
const BITWISE_OPS: &[&str] = &["&", "|", "^"];
const SHIFT_OPS: &[&str] = &["<<", ">>"];

/// `require` predicate shapes — `a` and `b` are the operands the
/// template exposes as local variables before the `require` call.
const PREDICATES: &[&str] = &[
    "a < b",
    "a > b",
    "a <= b",
    "a >= b",
    "a == b",
    "a != b",
    "(a & b) != 0",
    "(a ^ b) != 0",
];

fn pick<'b>(u: &mut Unstructured<'_>, choices: &[&'b str]) -> arbitrary::Result<&'b str> {
    Ok(*u.choose(choices)?)
}

// ── Common header ──────────────────────────────────────────────────────

fn header(s: &mut String, name: &str) {
    let _ = writeln!(s, "// SPDX-License-Identifier: MIT");
    let _ = writeln!(s, "pragma solidity ^0.8;");
    let _ = writeln!(s, "contract {name} {{");
}

fn footer(s: &mut String) {
    let _ = writeln!(s, "}}");
}

// ── Templates ──────────────────────────────────────────────────────────

fn render_srem(name: &str) -> String {
    let mut s = String::with_capacity(512);
    header(&mut s, name);
    let _ = writeln!(s, "    int256 public slot0;");
    let _ = writeln!(s, "    constructor(uint256 seed) {{ slot0 = int256(seed); }}");
    let _ = writeln!(s, "    function fn_0(uint256 arg) external returns (int256) {{");
    let _ = writeln!(s, "        int256 result = slot0 % int256(arg);");
    let _ = writeln!(s, "        slot0 = result;");
    let _ = writeln!(s, "        return result;");
    let _ = writeln!(s, "    }}");
    footer(&mut s);
    s
}

fn render_arith_chain(u: &mut Unstructured<'_>, name: &str) -> arbitrary::Result<String> {
    let op_a = pick(u, SIGNED_BIN_OPS)?;
    let op_b = pick(u, SIGNED_BIN_OPS)?;
    let op_c = pick(u, SIGNED_BIN_OPS)?;
    let mut s = String::with_capacity(640);
    header(&mut s, name);
    let _ = writeln!(s, "    int256 public s0;");
    let _ = writeln!(s, "    int256 public s1;");
    let _ = writeln!(s, "    constructor(uint256 seed) {{");
    let _ = writeln!(s, "        s0 = int256(seed);");
    let _ = writeln!(s, "        s1 = int256(seed ^ 0x55);");
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "    function fn_0(uint256 arg) external returns (int256) {{");
    let _ = writeln!(s, "        int256 a = int256(arg);");
    // Div/mod by zero revert identically on both backends — fine.
    let _ = writeln!(s, "        int256 mix = s1 {op_a} a;");
    let _ = writeln!(s, "        int256 r = s0 {op_b} mix;");
    let _ = writeln!(s, "        s0 = s0 {op_c} r;");
    let _ = writeln!(s, "        s1 = r;");
    let _ = writeln!(s, "        return r;");
    let _ = writeln!(s, "    }}");
    footer(&mut s);
    Ok(s)
}

fn render_unchecked_arith(u: &mut Unstructured<'_>, name: &str) -> arbitrary::Result<String> {
    let op_a = pick(u, UNSIGNED_BIN_OPS)?;
    let op_b = pick(u, UNSIGNED_BIN_OPS)?;
    let mut s = String::with_capacity(512);
    header(&mut s, name);
    let _ = writeln!(s, "    uint256 public slot;");
    let _ = writeln!(s, "    constructor(uint256 seed) {{ slot = seed; }}");
    let _ = writeln!(s, "    function fn_0(uint256 arg) external returns (uint256) {{");
    let _ = writeln!(s, "        unchecked {{");
    // `%` and `/` revert on zero divisor even inside unchecked.
    let _ = writeln!(s, "            uint256 r = slot {op_a} arg;");
    let _ = writeln!(s, "            r = r {op_b} (arg | 1);");
    let _ = writeln!(s, "            slot = r;");
    let _ = writeln!(s, "            return r;");
    let _ = writeln!(s, "        }}");
    let _ = writeln!(s, "    }}");
    footer(&mut s);
    Ok(s)
}

fn render_mapping(name: &str) -> String {
    let mut s = String::with_capacity(512);
    header(&mut s, name);
    let _ = writeln!(s, "    mapping(uint256 => uint256) public m;");
    let _ = writeln!(s, "    uint256 public lastKey;");
    let _ = writeln!(s, "    constructor(uint256 seed) {{ lastKey = seed; }}");
    let _ = writeln!(s, "    function fn_0(uint256 arg) external returns (uint256) {{");
    let _ = writeln!(s, "        uint256 key = arg ^ lastKey;");
    let _ = writeln!(s, "        unchecked {{ m[key] = m[key] + 1; }}");
    let _ = writeln!(s, "        lastKey = key;");
    let _ = writeln!(s, "        return m[key];");
    let _ = writeln!(s, "    }}");
    footer(&mut s);
    s
}

fn render_dyn_array(name: &str) -> String {
    let mut s = String::with_capacity(640);
    header(&mut s, name);
    let _ = writeln!(s, "    uint256[] public arr;");
    let _ = writeln!(s, "    constructor(uint256 seed) {{ arr.push(seed); }}");
    let _ = writeln!(s, "    function fn_0(uint256 arg) external returns (uint256) {{");
    // 64-element cap on push so growth doesn't OOG either backend.
    let _ = writeln!(s, "        if ((arg & 1) == 0 && arr.length < 64) {{");
    let _ = writeln!(s, "            arr.push(arg);");
    let _ = writeln!(s, "        }} else if (arr.length > 0) {{");
    let _ = writeln!(s, "            uint256 idx = arg % arr.length;");
    let _ = writeln!(s, "            arr[idx] = arr[idx] ^ arg;");
    let _ = writeln!(s, "        }}");
    let _ = writeln!(s, "        return arr.length == 0 ? 0 : arr[arr.length - 1];");
    let _ = writeln!(s, "    }}");
    footer(&mut s);
    s
}

fn render_require_guard(u: &mut Unstructured<'_>, name: &str) -> arbitrary::Result<String> {
    let predicate = pick(u, PREDICATES)?;
    let op = pick(u, UNSIGNED_BIN_OPS)?;
    let mut s = String::with_capacity(512);
    header(&mut s, name);
    let _ = writeln!(s, "    uint256 public slot;");
    let _ = writeln!(s, "    constructor(uint256 seed) {{ slot = seed; }}");
    let _ = writeln!(s, "    function fn_0(uint256 arg) external returns (uint256) {{");
    let _ = writeln!(s, "        uint256 a = slot;");
    let _ = writeln!(s, "        uint256 b = arg;");
    let _ = writeln!(s, "        require({predicate}, \"guard\");");
    let _ = writeln!(s, "        unchecked {{ slot = a {op} (b | 1); }}");
    let _ = writeln!(s, "        return slot;");
    let _ = writeln!(s, "    }}");
    footer(&mut s);
    Ok(s)
}

fn render_loop_accum(u: &mut Unstructured<'_>, name: &str) -> arbitrary::Result<String> {
    let op = pick(u, BITWISE_OPS)?;
    let mut s = String::with_capacity(640);
    header(&mut s, name);
    let _ = writeln!(s, "    uint256 public acc;");
    let _ = writeln!(s, "    constructor(uint256 seed) {{ acc = seed; }}");
    let _ = writeln!(s, "    function fn_0(uint256 arg) external returns (uint256) {{");
    // Cap at 31 iterations (arg & 0x1F) so gas spend stays comparable
    // on both backends — adversarial growth would mask backend bugs.
    let _ = writeln!(s, "        uint256 bound = arg & 0x1F;");
    let _ = writeln!(s, "        uint256 a = acc;");
    let _ = writeln!(s, "        for (uint256 i = 0; i < bound; i++) {{");
    let _ = writeln!(s, "            unchecked {{ a = (a {op} (arg + i)) + i; }}");
    let _ = writeln!(s, "        }}");
    let _ = writeln!(s, "        acc = a;");
    let _ = writeln!(s, "        return a;");
    let _ = writeln!(s, "    }}");
    footer(&mut s);
    Ok(s)
}

fn render_bitwise(u: &mut Unstructured<'_>, name: &str) -> arbitrary::Result<String> {
    let op_a = pick(u, BITWISE_OPS)?;
    let op_b = pick(u, BITWISE_OPS)?;
    let shift = pick(u, SHIFT_OPS)?;
    let mut s = String::with_capacity(512);
    header(&mut s, name);
    let _ = writeln!(s, "    uint256 public slot;");
    let _ = writeln!(s, "    constructor(uint256 seed) {{ slot = seed; }}");
    let _ = writeln!(s, "    function fn_0(uint256 arg) external returns (uint256) {{");
    // Cap shift at 8 bits — larger shifts saturate to 0, no coverage signal.
    let _ = writeln!(s, "        uint256 sh = arg & 0xff;");
    let _ = writeln!(s, "        uint256 r = (slot {op_a} arg) {op_b} (arg {shift} sh);");
    let _ = writeln!(s, "        slot = r;");
    let _ = writeln!(s, "        return r;");
    let _ = writeln!(s, "    }}");
    footer(&mut s);
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_unstructured() -> Vec<u8> {
        (0..4096u32)
            .flat_map(|i| i.wrapping_mul(1103515245).wrapping_add(12345).to_le_bytes())
            .collect()
    }

    #[test]
    fn every_template_renders() {
        let tape = seed_unstructured();
        for &kind in TemplateKind::ALL {
            let mut u = Unstructured::new(&tape);
            let rendered = render(kind, &mut u).expect("render");
            assert!(rendered.source.contains("contract "));
            assert!(rendered.source.contains("fn_0("));
            assert!(rendered.source.contains("constructor"));
            assert!(
                rendered.name.starts_with("Fuzz") && rendered.name.contains('_'),
                "name shape: {}",
                rendered.name
            );
        }
    }

    #[test]
    fn srem_template_uses_modulo() {
        let s = render_srem("FuzzSrem_deadbeef");
        assert!(s.contains("slot0 % int256(arg)"));
        assert!(s.contains("int256 public slot0"));
    }

    /// Sanity-check that every template is solc-accepted. Without it,
    /// a template typo would surface as a libFuzzer crash storm
    /// misattributed to the backend.
    #[test]
    #[ignore = "requires solc on PATH"]
    fn every_template_compiles() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let tape = seed_unstructured();
        for &kind in TemplateKind::ALL {
            let mut u = Unstructured::new(&tape);
            let rendered = render(kind, &mut u).expect("render");
            let standard_json = serde_json::json!({
                "language": "Solidity",
                "sources": { "fuzz.sol": { "content": rendered.source } },
                "settings": { "outputSelection": { "*": { "*": ["evm.bytecode.object"] } } }
            });
            let mut child = Command::new("solc")
                .args(["--standard-json"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn solc");
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(standard_json.to_string().as_bytes())
                .expect("write to solc stdin");
            let output = child.wait_with_output().expect("wait solc");
            assert!(
                output.status.success(),
                "solc rejected {:?}:\n{}",
                kind,
                String::from_utf8_lossy(&output.stderr),
            );
            // solc exits 0 even on `Error:` — has to be detected via
            // the `errors[].severity == "error"` field of the JSON.
            let stdout = String::from_utf8_lossy(&output.stdout);
            let parsed: serde_json::Value =
                serde_json::from_str(&stdout).expect("solc stdout is json");
            if let Some(errors) = parsed.get("errors").and_then(|e| e.as_array()) {
                let fatal: Vec<&serde_json::Value> = errors
                    .iter()
                    .filter(|e| e.get("severity").and_then(|s| s.as_str()) == Some("error"))
                    .collect();
                assert!(
                    fatal.is_empty(),
                    "solc errors on {:?}:\nsource:\n{}\nerrors: {:#?}",
                    kind,
                    rendered.source,
                    fatal,
                );
            }
        }
    }
}
