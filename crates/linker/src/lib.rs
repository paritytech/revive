use std::{ffi::CString, fs};

use inkwell::{context::Context, memory_buffer::MemoryBuffer, module::Module};
use lld_sys::LLDELFLink;
use revive_builtins::COMPILER_RT;

const LINKER_SCRIPT: &str = r#"
SECTIONS {
    .text : { KEEP(*(.text.polkavm_export)) *(.text .text.*) }
}"#;

fn invoke_lld(cmd_args: &[&str]) -> bool {
    let c_strings = cmd_args
        .iter()
        .map(|arg| CString::new(*arg).expect("ld.lld args should not contain null bytes"))
        .collect::<Vec<_>>();

    let args: Vec<*const libc::c_char> = c_strings.iter().map(|arg| arg.as_ptr()).collect();

    unsafe { LLDELFLink(args.as_ptr(), args.len()) == 0 }
}

fn polkavm_linker(code: &[u8]) -> Vec<u8> {
    let mut config = polkavm_linker::Config::default();
    config.set_strip(true);

    match polkavm_linker::program_from_elf(config, code) {
        Ok(blob) => blob.as_bytes().to_vec(),
        Err(reason) => panic!("polkavm linker failed: {}", reason),
    }
}

pub fn link(input: &[u8]) -> Vec<u8> {
    let dir = tempfile::tempdir().expect("failed to create temp directory for linking");
    let output_path = dir.path().join("out.so");
    let object_path = dir.path().join("out.o");
    let linker_script_path = dir.path().join("linker.ld");
    let compiler_rt_path = dir.path().join("libclang_rt.builtins-riscv32.a");

    fs::write(&object_path, input).unwrap_or_else(|msg| panic!("{msg} {object_path:?}"));

    fs::write(&linker_script_path, LINKER_SCRIPT)
        .unwrap_or_else(|msg| panic!("{msg} {linker_script_path:?}"));

    fs::write(&compiler_rt_path, COMPILER_RT)
        .unwrap_or_else(|msg| panic!("{msg} {compiler_rt_path:?}"));

    let ld_args = [
        "ld.lld",
        "--error-limit=0",
        "--relocatable",
        "--emit-relocs",
        "--no-relax",
        "--gc-sections",
        "--library-path",
        dir.path().to_str().expect("should be utf8"),
        "--library",
        "clang_rt.builtins-riscv32",
        linker_script_path.to_str().expect("should be utf8"),
        object_path.to_str().expect("should be utf8"),
        "-o",
        output_path.to_str().expect("should be utf8"),
    ];

    assert!(!invoke_lld(&ld_args), "ld.lld failed");

    fs::copy(&object_path, "/tmp/out.o").unwrap();
    fs::copy(&output_path, "/tmp/out.so").unwrap();
    fs::copy(&linker_script_path, "/tmp/linkder.ld").unwrap();

    let blob = fs::read(&output_path).expect("ld.lld should produce output");
    polkavm_linker(&blob)
}

pub fn libraries(context: &Context) -> Vec<Module<'_>> {
    let guest_bitcode = include_bytes!("../polkavm_guest.bc");
    let imports = MemoryBuffer::create_from_memory_range(guest_bitcode, "guest_bc");

    vec![Module::parse_bitcode_from_buffer(&imports, context).unwrap()]
}
