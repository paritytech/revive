use std::{env, fs, path::Path, process::Command};

fn main() {
    println!(
        "cargo:rerun-if-env-changed={}",
        revive_build_utils::REVIVE_LLVM_HOST_PREFIX
    );

    let lib = "stdlib.bc";
    let out_dir = env::var_os("OUT_DIR").expect("env should have $OUT_DIR");
    let bitcode_path = Path::new(&out_dir).join(lib);
    let llvm_as = revive_build_utils::llvm_host_tool("llvm-as");
    let output = Command::new(llvm_as)
        .args([
            "-o",
            bitcode_path.to_str().expect("$OUT_DIR should be UTF-8"),
            "stdlib.ll",
        ])
        .output()
        .unwrap_or_else(|error| panic!("failed to execute llvm-as: {}", error));

    assert!(
        output.status.success(),
        "failed to assemble the stdlib: {:?}",
        output
    );

    let bitcode = fs::read(bitcode_path).expect("bitcode should have been built");
    let len = bitcode.len();
    let src_path = Path::new(&out_dir).join("stdlib.rs");
    let src = format!("pub static BITCODE: &[u8; {len}] = include_bytes!(\"{lib}\");");
    fs::write(src_path, src).expect("should be able to write in $OUT_DIR");

    println!("cargo:rerun-if-changed=stdlib.ll");
    println!("cargo:rerun-if-changed=build.rs");
}
