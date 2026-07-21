//! LLM-driven Solidity seed-corpus generator for the revive differential
//! fuzzer (`solidity_source_differential`).
//!
//! For each built-in "risk area" it prompts an LLM for a self-contained
//! Solidity contract matching the fuzzer's wire shape, validates it (structural
//! check + optional real `solc` compile), and writes accepted contracts as
//! content-addressed `.sol` files into the output directory. Seed that
//! directory into the fuzzer's corpus and let the byte-level engines mutate it.
//!
//! Default provider is the Claude Code CLI (`claude -p`), which reuses Claude
//! Code's existing auth — no API key required. `--provider claude` uses the
//! Anthropic Messages API (`ANTHROPIC_API_KEY`); `--provider custom` targets
//! any OpenAI-compatible chat-completions endpoint.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use serde_json::json;
use sha2::{Digest, Sha256};

/// Anthropic Messages API endpoint.
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
/// Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Default Claude model — override with `--model`.
const DEFAULT_CLAUDE_MODEL: &str = "claude-sonnet-5";
/// Default upper bound on generated tokens per contract.
const DEFAULT_MAX_TOKENS: u32 = 2048;
/// Length of the hex digest used for content-addressed filenames.
const DIGEST_HEX_LEN: usize = 16;

/// Contract shapes worth generating — each targets a distinct resolc lowering
/// path where EVM/PVM divergences have historically surfaced.
const RISK_AREAS: &[&str] = &[
    "signed int256 arithmetic including division and modulo (srem) with INT_MIN and -1 operands",
    "unchecked { } blocks wrapping overflowing/underflowing add, sub, mul",
    "storage mappings and dynamic uint256[] arrays with read-after-write aliasing",
    "a bounded for-loop accumulator whose trip count derives from the argument (masked to <= 31)",
    "bitwise composition: and, or, xor, and shifts by amounts near and above 256",
    "require/assert guards with non-trivial boolean predicates over the argument and storage",
    "nested and chained arithmetic expressions that stress the optimizer's constant folding",
];

/// System prompt: pins the wire shape and forbids environment-dependent
/// behaviour that would produce false EVM/PVM divergences.
const SYSTEM_PROMPT: &str = "\
You generate Solidity contracts used as inputs to a differential compiler \
fuzzer that compiles each contract with two backends and compares execution. \
Follow these rules EXACTLY:\n\
- Output ONLY Solidity source code. No prose, no explanation.\n\
- Start with `// SPDX-License-Identifier: MIT` and `pragma solidity ^0.8;`.\n\
- Define exactly ONE entrypoint contract named `Fuzz`.\n\
- It MUST declare `constructor(uint256 seed)` and \
`function fn_0(uint256 arg) external returns (uint256)`.\n\
- `fn_0` must read and/or write contract storage and return a deterministic \
uint256 derived from the computation.\n\
- The contract must be self-contained and compile with no imports.\n\
- Behaviour must be PURELY deterministic and depend ONLY on `seed`, `arg`, and \
contract storage.\n\
- FORBIDDEN (they cause false divergences): block.*, tx.*, msg.value, \
msg.sender, address(this), .balance, gasleft(), blockhash, keccak256 over \
environment data, external calls, delegatecall, create/create2, selfdestruct, \
inline assembly reading environment, randomness, timestamps.\n\
- Prefer arithmetic, storage, memory, bitwise, and control-flow logic.\n\
- Keep it under ~60 lines.";

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Provider {
    /// Claude Code CLI (default). Uses the `claude` binary's existing auth —
    /// no API key required. Requires Claude Code to be installed on PATH.
    ClaudeCode,
    /// Anthropic Messages API. Key: `ANTHROPIC_API_KEY`.
    Claude,
    /// Any OpenAI-compatible `/chat/completions` endpoint. Key: `LLM_API_KEY`.
    Custom,
}

#[derive(Parser, Debug)]
#[command(about = "Generate an LLM Solidity seed corpus for the revive fuzzer")]
struct Args {
    /// Output directory for generated `.sol` files.
    #[arg(long, default_value = "llm-corpus")]
    out: PathBuf,

    /// Number of contracts to request per risk area.
    #[arg(long, default_value_t = 5)]
    count: usize,

    /// LLM provider.
    #[arg(long, value_enum, default_value_t = Provider::ClaudeCode)]
    provider: Provider,

    /// Model id. Optional for claude-code (uses its configured default);
    /// defaults to a Claude model for the `claude` API provider.
    #[arg(long)]
    model: Option<String>,

    /// Endpoint URL for `--provider custom` (else `LLM_ENDPOINT`).
    #[arg(long)]
    endpoint: Option<String>,

    /// Max tokens per generation.
    #[arg(long, default_value_t = DEFAULT_MAX_TOKENS)]
    max_tokens: u32,

    /// Extra guidance appended to every prompt — e.g. an uncovered codepath
    /// (Component 4: steer generation at a coverage gap).
    #[arg(long)]
    focus: Option<String>,

    /// Skip the real `solc` compile check (structural validation only).
    #[arg(long)]
    no_solc: bool,

    /// Sample existing contracts from this directory and instruct the model to
    /// generate NEW ones that are structurally different — the feedback loop's
    /// "diversify against the previous corpus" mode.
    #[arg(long)]
    avoid_dir: Option<PathBuf>,

    /// How many existing contracts to show the model as "avoid these".
    #[arg(long, default_value_t = 4)]
    avoid_count: usize,

    /// Per-sample character cap when showing existing contracts.
    #[arg(long, default_value_t = 800)]
    avoid_maxlen: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();
    std::fs::create_dir_all(&args.out)
        .with_context(|| format!("creating output dir {}", args.out.display()))?;

    let solc = !args.no_solc && solc_available();
    if !args.no_solc && !solc {
        eprintln!("warning: `solc` not found on PATH — using structural validation only");
    }

    let avoid = args
        .avoid_dir
        .as_deref()
        .map(|dir| sample_avoid(dir, args.avoid_count, args.avoid_maxlen))
        .unwrap_or_default();
    if !avoid.is_empty() {
        eprintln!("diversifying against {} existing contract(s)", avoid.len());
    }

    let agent = ureq::AgentBuilder::new().build();
    let mut accepted = 0usize;
    let mut rejected = 0usize;

    for area in RISK_AREAS {
        for _ in 0..args.count {
            let user_prompt = build_user_prompt(area, args.focus.as_deref(), &avoid);
            let raw = match generate(&agent, &args, &user_prompt) {
                Ok(text) => text,
                Err(error) => {
                    eprintln!("request failed: {error:#}");
                    continue;
                }
            };
            let source = extract_code(&raw);

            if !structural_ok(&source) {
                rejected += 1;
                continue;
            }
            if solc && !solc_compiles(&source) {
                rejected += 1;
                continue;
            }

            match write_dedup(&args.out, &source) {
                Ok(true) => {
                    accepted += 1;
                    eprintln!("accepted [{accepted}] ({area})");
                }
                Ok(false) => {} // duplicate content, already present
                Err(error) => eprintln!("write failed: {error:#}"),
            }
        }
    }

    eprintln!(
        "done: {accepted} written, {rejected} rejected → {}",
        args.out.display()
    );
    Ok(())
}

/// Compose the per-contract user prompt from a risk area, optional focus, and
/// optional "avoid these existing contracts" samples (diversify mode).
fn build_user_prompt(area: &str, focus: Option<&str>, avoid: &[String]) -> String {
    let mut prompt = format!(
        "Generate one Solidity contract that a differential compiler fuzzer \
         would find interesting for this scenario: {area}."
    );
    if let Some(focus) = focus {
        prompt.push_str(&format!(
            " Additionally, try to exercise this specific code path or feature: {focus}."
        ));
    }
    if !avoid.is_empty() {
        prompt.push_str(
            "\n\nThe following contracts have already been tried. Generate one that is \
             STRUCTURALLY DIFFERENT from all of them — different control flow, data \
             structures, opcodes, and edge cases; not a renamed variation:\n",
        );
        for (index, existing) in avoid.iter().enumerate() {
            prompt.push_str(&format!("\n--- existing #{index} ---\n{existing}\n"));
        }
    }
    prompt
}

/// Sample up to `count` already-generated contracts from `dir` to show the model
/// as "avoid these". Stride-sampled across the directory with a time-varying
/// offset so successive rounds surface different examples; each truncated to
/// `maxlen` chars to bound prompt size. Only files that look like Solidity
/// (contain `contract `) are used, so a mixed fuzzer corpus works too.
fn sample_avoid(dir: &Path, count: usize, maxlen: usize) -> Vec<String> {
    if count == 0 {
        return Vec::new();
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .collect();
    if files.is_empty() {
        return Vec::new();
    }
    files.sort();
    let total = files.len();
    let offset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos() as usize)
        .unwrap_or(0)
        % total;
    let stride = (total / count).max(1);
    let mut samples = Vec::with_capacity(count);
    let mut step = 0;
    while samples.len() < count && step < total {
        let index = (offset + step * stride) % total;
        step += 1;
        if let Ok(mut source) = std::fs::read_to_string(&files[index]) {
            // Skip binary/heavily-mutated corpus files: a NUL is a valid UTF-8
            // char (so `read_to_string` accepts it) but cannot be passed as a
            // process arg, and such files make poor "avoid" examples anyway.
            if source.contains("contract ") && !source.contains('\0') {
                if source.len() > maxlen {
                    let mut end = maxlen;
                    while end > 0 && !source.is_char_boundary(end) {
                        end -= 1;
                    }
                    source.truncate(end);
                }
                samples.push(source);
            }
        }
    }
    samples
}

/// Dispatch a single generation request to the configured provider.
fn generate(agent: &ureq::Agent, args: &Args, user_prompt: &str) -> Result<String> {
    match args.provider {
        Provider::ClaudeCode => call_claude_code(args.model.as_deref(), user_prompt),
        Provider::Claude => {
            let key = std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY is not set")?;
            let model = args.model.as_deref().unwrap_or(DEFAULT_CLAUDE_MODEL);
            call_claude(agent, &key, model, user_prompt, args.max_tokens)
        }
        Provider::Custom => {
            let key = std::env::var("LLM_API_KEY").context("LLM_API_KEY is not set")?;
            let endpoint = args
                .endpoint
                .clone()
                .or_else(|| std::env::var("LLM_ENDPOINT").ok())
                .context("--endpoint or LLM_ENDPOINT is required for --provider custom")?;
            let model = args
                .model
                .clone()
                .or_else(|| std::env::var("LLM_MODEL").ok())
                .context("--model or LLM_MODEL is required for --provider custom")?;
            call_openai(agent, &endpoint, &key, &model, user_prompt, args.max_tokens)
        }
    }
}

/// Generate via the Claude Code CLI (`claude -p`), reusing its existing auth
/// so no API key is needed. The system prompt is folded into the user prompt
/// for version-robustness (avoids relying on `--append-system-prompt`).
fn call_claude_code(model: Option<&str>, user_prompt: &str) -> Result<String> {
    // A process arg cannot contain NUL; strip any (e.g. from mutated corpus
    // samples) so the spawn never fails on it.
    let combined = format!("{SYSTEM_PROMPT}\n\n{user_prompt}").replace('\0', "");
    let mut command = Command::new("claude");
    command
        .arg("--print")
        .arg("--output-format")
        .arg("text")
        .arg(&combined);
    if let Some(model) = model {
        command.arg("--model").arg(model);
    }
    let output = command
        .output()
        .context("failed to run `claude` CLI — is Claude Code installed and on PATH?")?;
    if !output.status.success() {
        bail!(
            "claude CLI exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if text.trim().is_empty() {
        bail!("claude CLI returned empty output");
    }
    Ok(text)
}

/// Call the Anthropic Messages API and return the concatenated text blocks.
fn call_claude(
    agent: &ureq::Agent,
    key: &str,
    model: &str,
    user_prompt: &str,
    max_tokens: u32,
) -> Result<String> {
    let body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": SYSTEM_PROMPT,
        "messages": [{ "role": "user", "content": user_prompt }],
    });
    let value = post_json(
        agent
            .post(ANTHROPIC_URL)
            .set("x-api-key", key)
            .set("anthropic-version", ANTHROPIC_VERSION)
            .set("content-type", "application/json"),
        body,
    )?;
    let text: String = value["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    if text.is_empty() {
        bail!("empty response from Claude: {value}");
    }
    Ok(text)
}

/// Call an OpenAI-compatible chat-completions endpoint.
fn call_openai(
    agent: &ureq::Agent,
    endpoint: &str,
    key: &str,
    model: &str,
    user_prompt: &str,
    max_tokens: u32,
) -> Result<String> {
    let body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": user_prompt },
        ],
    });
    let value = post_json(
        agent
            .post(endpoint)
            .set("authorization", &format!("Bearer {key}"))
            .set("content-type", "application/json"),
        body,
    )?;
    value["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("no message content in response: {value}"))
}

/// POST JSON and surface API error bodies (ureq treats non-2xx as `Err`).
fn post_json(request: ureq::Request, body: serde_json::Value) -> Result<serde_json::Value> {
    match request.send_json(body) {
        Ok(response) => response.into_json().context("decoding JSON response"),
        Err(ureq::Error::Status(code, response)) => {
            let detail = response.into_string().unwrap_or_default();
            bail!("API returned HTTP {code}: {detail}")
        }
        Err(error) => Err(error).context("HTTP request failed"),
    }
}

/// Extract the code body from a model response, stripping a Markdown fence if
/// present (```/```solidity ... ```).
fn extract_code(response: &str) -> String {
    let Some(start) = response.find("```") else {
        return response.trim().to_owned();
    };
    let after = &response[start + 3..];
    // Drop an optional language tag on the fence's opening line.
    let after = match after.find('\n') {
        Some(newline) => &after[newline + 1..],
        None => after,
    };
    match after.find("```") {
        Some(end) => after[..end].trim().to_owned(),
        None => after.trim().to_owned(),
    }
}

/// Cheap structural gate mirroring the fuzzer's wire-shape expectations.
fn structural_ok(source: &str) -> bool {
    source.contains("contract ")
        && source.contains("fn_0(uint256")
        && source.contains("constructor(uint256")
}

/// Whether a `solc` binary is reachable.
fn solc_available() -> bool {
    Command::new("solc")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Compile-check a contract with the real `solc` (EVM bytecode). Only contracts
/// solc accepts are useful seeds — a solc-rejected input is skipped by the
/// oracle as a generator bug, whereas a resolc-only failure is a real find and
/// is intentionally NOT filtered here.
fn solc_compiles(source: &str) -> bool {
    let path = std::env::temp_dir().join(format!("llm-corpus-{}.sol", digest_hex(source)));
    if std::fs::write(&path, source).is_err() {
        return false;
    }
    let ok = Command::new("solc")
        .arg("--bin")
        .arg(&path)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    let _ = std::fs::remove_file(&path);
    ok
}

/// Write `source` under a content-addressed name. Returns `false` if an
/// identical contract is already present.
fn write_dedup(dir: &Path, source: &str) -> Result<bool> {
    let path = dir.join(format!("{}.sol", digest_hex(source)));
    if path.exists() {
        return Ok(false);
    }
    std::fs::write(&path, source).with_context(|| format!("writing {}", path.display()))?;
    Ok(true)
}

/// Truncated hex SHA-256 of the source, for content addressing.
fn digest_hex(source: &str) -> String {
    let digest = Sha256::digest(source.as_bytes());
    let mut hex = String::with_capacity(DIGEST_HEX_LEN);
    for byte in digest.iter().take(DIGEST_HEX_LEN / 2) {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}
