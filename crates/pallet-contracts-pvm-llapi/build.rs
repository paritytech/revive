use std::{env, fs, path::Path, process::Command};

fn compile(bitcode_path: &str) {
    let output = Command::new("clang")
        .args([
            "--target=riscv32",
            "-Xclang",
            "-triple=riscv32-unknown-unknown-elf",
            "-march=rv32em",
            "-mabi=ilp32e",
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

    let bitcode = fs::read(&bitcode_path).expect("bitcode should have been built");
    let len = bitcode.len();

    // Copying the polkavm_guest.bc file to the current directory
    // for now, I will change this to copy to the output directory
    // specified by the user.
    // e.g:
    // `cargo run Storage.sol -O3 --bin --output-dir './out'` 
    // should copy the bitcode to `./out`. 
    let current_dir = env::current_dir().expect("should be able to get current directory");
    let output_path = current_dir.join(lib);
    fs::copy(&bitcode_path, &output_path).expect("should be able to copy bitcode to current directory");

    let src_path = Path::new(&out_dir).join("polkavm_guest.rs");
    let src = format!("pub static BITCODE: &[u8; {len}] = include_bytes!(\"{lib}\");");
    fs::write(src_path, src).expect("should be able to write in $OUT_DIR");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/polkavm_guest.c");
    println!("cargo:rerun-if-changed=src/polkavm_guest.h");
}
