use std::{
    env,
    path::{Path, PathBuf},
};

const LLVM_LINK_PREFIX: &str = "LLVM_LINK_PREFIX";

fn locate_llvm_config() -> PathBuf {
    let prefix = env::var_os(LLVM_LINK_PREFIX)
        .map(|p| PathBuf::from(p).join("bin"))
        .unwrap_or_default();
    prefix.join("llvm-config")
}

fn llvm_config(llvm_config_path: &Path, arg: &str) -> String {
    let output = std::process::Command::new(llvm_config_path)
        .args([arg])
        .output()
        .unwrap_or_else(|_| panic!("`llvm-config {arg}` failed"));

    String::from_utf8(output.stdout)
        .unwrap_or_else(|_| panic!("output of `llvm-config {arg}` should be utf8"))
}

fn set_rustc_link_flags(llvm_config_path: &Path) {
    println!(
        "cargo:rustc-link-search=native={}",
        llvm_config(llvm_config_path, "--libdir")
    );

    for lib in [
        "lldELF",
        "lldCommon",
        "lldMachO",
        "LLVMSupport",
        "LLVMLinker",
        "LLVMCore",
        "LLVMLTO",
        "LLVMTargetParser",
        "LLVMBinaryFormat",
        "LLVMDemangle",
        // Required by `llvm-sys`
        "LLVMRISCVDisassembler",
        "LLVMRISCVAsmParser",
        "LLVMRISCVCodeGen",
        "LLVMRISCVDesc",
        "LLVMRISCVInfo",
        "LLVMExecutionEngine",
        "LLVMOption",
        "LLVMMCDisassembler",
        "LLVMPasses",
        "LLVMHipStdPar",
        "LLVMCFGuard",
        "LLVMCoroutines",
        "LLVMipo",
        "LLVMVectorize",
        "LLVMInstrumentation",
        "LLVMFrontendOpenMP",
        "LLVMFrontendOffloading",
        "LLVMGlobalISel",
        "LLVMAsmPrinter",
        "LLVMSelectionDAG",
        "LLVMCodeGen",
        "LLVMTarget",
        "LLVMObjCARCOpts",
        "LLVMCodeGenTypes",
        "LLVMIRPrinter",
        "LLVMScalarOpts",
        "LLVMInstCombine",
        "LLVMAggressiveInstCombine",
        "LLVMTransformUtils",
        "LLVMBitWriter",
        "LLVMAnalysis",
        "LLVMProfileData",
        "LLVMDebugInfoDWARF",
        "LLVMObject",
        "LLVMMCParser",
        "LLVMIRReader",
        "LLVMAsmParser",
        "LLVMMC",
        "LLVMDebugInfoCodeView",
        "LLVMBitReader",
        "LLVMRemarks",
        "LLVMBitstreamReader",
    ] {
        println!("cargo:rustc-link-lib=static={lib}");
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "linux" {
        println!("cargo:rustc-link-lib=dylib=stdc++");
        println!("cargo:rustc-link-lib=tinfo");
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed={}", LLVM_LINK_PREFIX);

    let llvm_config_path = locate_llvm_config();

    llvm_config(&llvm_config_path, "--cxxflags")
        .split_whitespace()
        .fold(&mut cc::Build::new(), |builder, flag| builder.flag(flag))
        .flag("-Wno-unused-parameter")
        .cpp(true)
        .file("src/linker.cpp")
        .compile("liblinker.a");

    set_rustc_link_flags(&llvm_config_path);

    println!("cargo:rerun-if-changed=build.rs");
}
