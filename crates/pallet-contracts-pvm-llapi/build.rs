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

fn compile(bitcode_path: &str) {
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
            "src/polkavm_guest.c",
            "-o",
            bitcode_path,
        ])
        .output()
        .expect("should be able to invoke C clang");

    assert!(
        output.status.success(),
        "failed to compile the PolkaVM C API: {:?}",
        output
    );
}

fn main() {
    let out_dir = env::var_os("OUT_DIR").expect("env should have $OUT_DIR");
    let lib = "polkavm_guest.bc";
    let bitcode_path = Path::new(&out_dir).join(lib);
    compile(bitcode_path.to_str().expect("$OUT_DIR should be UTF-8"));

    let bitcode = fs::read(bitcode_path).expect("bitcode should have been built");
    let len = bitcode.len();
    let src_path = Path::new(&out_dir).join("polkavm_guest.rs");
    let src = format!("pub static BITCODE: &[u8; {len}] = include_bytes!(\"{lib}\");");
    fs::write(src_path, src).expect("should be able to write in $OUT_DIR");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/polkavm_guest.c");
    println!("cargo:rerun-if-changed=src/polkavm_guest.h");
}
