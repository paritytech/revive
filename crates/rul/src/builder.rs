//! The revive rust backend builder module.

const CARGO_TOML: &str = r#"
[package]
name = "contracts"
publish = false
version = "1.0.0"
edition = "2021"

# Make sure this is not included into the workspace
[workspace]

# Binary targets are injected dynamically by the build script.
[[bin]]

# All paths are injected dynamically by the build script.
[dependencies]
uapi = { version = "0.7.0", package = 'pallet-revive-uapi', features = ["unstable-hostfn"], default-features = false }
hex-literal = { version = "0.4.1", default-features = false }
polkavm-derive = { version = "0.27.0" }

[profile.release]
opt-level = z
lto = true
codegen-units = 1
"#;

const HEADER: &str = r#"
#![no_std]
#![no_main]
include!("../panic_handler.rs");

use uapi::{HostFn, HostFnImpl as api, ReturnFlags};

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
	// SAFETY: The unimp instruction is guaranteed to trap
	unsafe {
		core::arch::asm!("unimp");
		core::hint::unreachable_unchecked();
	}
}

/// The emulated linear EVM heap memory size.
pub const MEMORY_SIZE: usize = 1024 * 64;

/// The emulated linear EVM heap memory size.
pub const MEMORY: [u8; MEMORY_SIZE] = [0; MEMORY_SIZE];
"#;

const EXPORT_FUNCTION: &str = r#"
#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn "#;

fn emit(constructor_code: &str, runtime_code: &str) -> String {
    let mut buffer = String::from(HEADER);
    buffer.reserve(
        constructor_code.len() + runtime_code.len() + HEADER.len() + EXPORT_FUNCTION.len() * 2,
    );

    buffer.push_str(EXPORT_FUNCTION);
    buffer.push_str("deploy() {");
    buffer.push_str(constructor_code);
    buffer.push_str("\n}");

    buffer.push_str(EXPORT_FUNCTION);
    buffer.push_str("call() {");
    buffer.push_str(runtime_code);
    buffer.push_str("\n}");

    buffer
}

/// Build a PVM blob.
pub fn build(constructor_code: &str, runtime_code: &str) -> Vec<u8> {
    let code = emit(constructor_code, runtime_code);
    todo!();
}
