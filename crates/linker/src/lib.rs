use std::{env, ffi::CString, fs};

use lld_sys::LLDELFLink;
use revive_builtins::COMPILER_RT;

const LINKER_SCRIPT: &str = r#"
SECTIONS {
    .text : { KEEP(*(.text.polkavm_export)) *(.text .text.*) }
}"#;

const BUILTINS_ARCHIVE_FILE: &str = "libclang_rt.builtins-riscv64.a";
const BUILTINS_LIB_NAME: &str = "clang_rt.builtins-riscv64";

fn invoke_lld(cmd_args: &[&str]) -> bool {
    let c_strings = cmd_args
        .iter()
        .map(|arg| CString::new(*arg).expect("ld.lld args should not contain null bytes"))
        .collect::<Vec<_>>();

    let args: Vec<*const libc::c_char> = c_strings.iter().map(|arg| arg.as_ptr()).collect();

    unsafe { LLDELFLink(args.as_ptr(), args.len()) == 0 }
}

pub fn polkavm_linker<T: AsRef<[u8]>>(code: T, strip_binary: bool) -> anyhow::Result<Vec<u8>> {
    let mut config = polkavm_linker::Config::default();
    config.set_strip(strip_binary);
    config.set_optimize(true);

    polkavm_linker::program_from_elf(config, code.as_ref())
        .map_err(|reason| anyhow::anyhow!("polkavm linker failed: {}", reason))
}

pub fn link<T: AsRef<[u8]>>(input: T) -> anyhow::Result<Vec<u8>> {
    let dir = tempfile::tempdir().expect("failed to create temp directory for linking");
    let output_path = dir.path().join("out.so");
    let object_path = dir.path().join("out.o");
    let linker_script_path = dir.path().join("linker.ld");
    let compiler_rt_path = dir.path().join(BUILTINS_ARCHIVE_FILE);

    fs::write(&object_path, input).map_err(|msg| anyhow::anyhow!("{msg} {object_path:?}"))?;

    if env::var("PVM_LINKER_DUMP_OBJ").is_ok() {
        fs::copy(&object_path, "/tmp/out.o")?;
    }

    fs::write(&linker_script_path, LINKER_SCRIPT)
        .map_err(|msg| anyhow::anyhow!("{msg} {linker_script_path:?}"))?;

    fs::write(&compiler_rt_path, COMPILER_RT)
        .map_err(|msg| anyhow::anyhow!("{msg} {compiler_rt_path:?}"))?;

    let ld_args = [
        "ld.lld",
        "--error-limit=0",
        "--relocatable",
        "--emit-relocs",
        "--no-relax",
        "--unique",
        "--gc-sections",
        "--library-path",
        dir.path().to_str().expect("should be utf8"),
        "--library",
        BUILTINS_LIB_NAME,
        linker_script_path.to_str().expect("should be utf8"),
        object_path.to_str().expect("should be utf8"),
        "-o",
        output_path.to_str().expect("should be utf8"),
    ];

    if invoke_lld(&ld_args) {
        return Err(anyhow::anyhow!("ld.lld failed"));
    }

    Ok(fs::read(&output_path)?)
}
