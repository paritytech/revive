use std::{env, fs, io::Read, path::Path, process::Command};

pub const BUILTINS_ARCHIVE_FILE: &str = "libclang_rt.builtins-riscv64.a";

fn main() {
    let mut llvm_lib_dir = String::new();

    Command::new("llvm-config")
        .args(["--libdir"])
        .output()
        .expect("llvm-config should be able to provide LD path")
        .stdout
        .as_slice()
        .read_to_string(&mut llvm_lib_dir)
        .expect("llvm-config output should be utf8");

    let mut lib_path = std::path::PathBuf::from(llvm_lib_dir.trim())
        .join("linux")
        .join(BUILTINS_ARCHIVE_FILE);
    if !lib_path.exists() {
        lib_path = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join(BUILTINS_ARCHIVE_FILE);
    }
    let archive = fs::read(lib_path).expect("clang builtins not found");

    let out_dir = env::var_os("OUT_DIR").expect("has OUT_DIR");
    let archive_path = Path::new(&out_dir).join(BUILTINS_ARCHIVE_FILE);
    let len = archive.len();
    std::fs::write(archive_path, &archive).expect("can write to OUT_DIR");

    let src_path = Path::new(&out_dir).join("compiler_rt.rs");
    let src = format!(
        "pub static COMPILER_RT: &[u8; {len}] = include_bytes!(\"{BUILTINS_ARCHIVE_FILE}\");"
    );
    fs::write(src_path, src).expect("can write to OUT_DIR");

    println!("cargo:rerun-if-changed=build.rs");
}
