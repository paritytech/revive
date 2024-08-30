use std::{env, fs, path::Path, process::Command};

#[cfg(not(feature = "riscv-64"))]
const TARGET_TRIPLE_FLAG: &str = "-triple=riscv32-unknown-unknown-elf";
#[cfg(feature = "riscv-64")]
const TARGET_TRIPLE_FLAG: &str = "-triple=riscv64-unknown-unknown-elf";

#[cfg(not(feature = "riscv-64"))]
const TARGET_FLAG: &str = "--target=riscv32";
#[cfg(feature = "riscv-64")]
const TARGET_FLAG: &str = "--target=riscv64";

#[cfg(not(feature = "riscv-64"))]
const TARGET_ARCH_FLAG: &str = "-march=rv32em";
#[cfg(feature = "riscv-64")]
const TARGET_ARCH_FLAG: &str = "-march=rv64em";

#[cfg(not(feature = "riscv-64"))]
const TARGET_ABI_FLAG: &str = "-mabi=ilp32e";
#[cfg(feature = "riscv-64")]
const TARGET_ABI_FLAG: &str = "-mabi=lp64e";

const IMPORTS_SOUCE: &str = "src/polkavm_imports.c";
const IMPORTS_BC: &str = "polkavm_imports.bc";
const IMPORTS_RUST: &str = "polkavm_imports.rs";

const EXPORTS_SOUCE: &str = "src/polkavm_exports.c";
const EXPORTS_BC: &str = "polkavm_exports.bc";
const EXPORTS_RUST: &str = "polkavm_exports.rs";

fn compile(source_path: &str, bitcode_path: &str) {
    let output = Command::new("clang")
        .args([
            TARGET_FLAG,
            "-Xclang",
            TARGET_TRIPLE_FLAG,
            TARGET_ARCH_FLAG,
            TARGET_ABI_FLAG,
            "-fno-exceptions",
            "-ffreestanding",
            "-Wall",
            "-fno-builtin",
            "-O3",
            "-emit-llvm",
            "-c",
            "-o",
            bitcode_path,
            source_path,
        ])
        .output()
        .expect("should be able to invoke C clang");

    assert!(
        output.status.success(),
        "failed to compile the PolkaVM C API: {:?}",
        output
    );
}

fn build_module(source_path: &str, bitcode_path: &str, rust_file: &str) {
    let out_dir = env::var_os("OUT_DIR").expect("env should have $OUT_DIR");
    let lib = Path::new(&out_dir).join(bitcode_path);
    compile(source_path, lib.to_str().expect("$OUT_DIR should be UTF-8"));

    let bitcode = fs::read(lib).expect("bitcode should have been built");
    let len = bitcode.len();
    let src_path = Path::new(&out_dir).join(rust_file);
    let src = format!("pub static BITCODE: &[u8; {len}] = include_bytes!(\"{bitcode_path}\");");
    fs::write(src_path, src).expect("should be able to write in $OUT_DIR");
}

fn main() {
    build_module(IMPORTS_SOUCE, IMPORTS_BC, IMPORTS_RUST);
    build_module(EXPORTS_SOUCE, EXPORTS_BC, EXPORTS_RUST);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/polkavm_imports.c");
    println!("cargo:rerun-if-changed=src/polkavm_exports.c");
    println!("cargo:rerun-if-changed=src/polkavm_guest.h");
}
