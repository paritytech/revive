use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub const BUILTINS_ARCHIVE_FILE: &str = "libclang_rt.builtins-riscv64.a";

fn main() {
    println!(
        "cargo:rerun-if-env-changed={}",
        revive_build_utils::REVIVE_LLVM_HOST_PREFIX
    );
    println!(
        "cargo:rerun-if-env-changed={}",
        revive_build_utils::REVIVE_LLVM_TARGET_PREFIX
    );

    // When cross-compiling, the riscv64 builtins live in the target LLVM,
    // not the host LLVM that `LLVM_SYS_221_PREFIX` points at.
    let llvm_lib_dir = match env::var_os(revive_build_utils::REVIVE_LLVM_TARGET_PREFIX) {
        Some(prefix) => PathBuf::from(prefix).join("lib"),
        None => revive_build_utils::llvm_lib_dir(),
    };

    let lib_path = llvm_lib_dir.join("unknown").join(BUILTINS_ARCHIVE_FILE);
    let archive = fs::read(&lib_path).expect("clang builtins not found");
    println!("cargo:rerun-if-changed={}", lib_path.display());

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
