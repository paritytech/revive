use std::{env, fs, io::Read, path::Path, process::Command};

pub const BUILTINS_ARCHIVE_FILE: &str = "libclang_rt.builtins-riscv64.a";

fn main() {
    println!(
        "cargo:rerun-if-env-changed={}",
        revive_build_utils::REVIVE_LLVM_HOST_PREFIX
    );

    let llvm_config = revive_build_utils::llvm_host_tool("llvm-config");
    let mut llvm_lib_dir = String::new();
    Command::new(&llvm_config)
        .arg("--libdir")
        .output()
        .unwrap_or_else(|_| {
            panic!(
                "{} should be able to provide LD path",
                llvm_config.display()
            )
        })
        .stdout
        .as_slice()
        .read_to_string(&mut llvm_lib_dir)
        .expect("llvm-config output should be utf8");

    let lib_path = revive_build_utils::llvm_lib_dir()
        .join("unknown")
        .join(BUILTINS_ARCHIVE_FILE);
    let archive = fs::read(&lib_path).expect("clang builtins not found");
    println!("cargo:rerun-if-env-changed={}", lib_path.display());

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
