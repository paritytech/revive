/// Link against the target environment's LLVM libraries if requested.
fn locate_llvm_config() -> std::path::PathBuf {
    match std::env::var(revive_llvm_builder::utils::REVIVE_LLVM_TARGET_PREFIX) {
        Ok(path) => std::path::PathBuf::from(path)
            .join("bin")
            .join("llvm-config"),
        _ => revive_llvm_builder::utils::llvm_host_tool("llvm-config"),
    }
}

fn llvm_config(arg: &str) -> String {
    let llvm_config = locate_llvm_config();
    let output = std::process::Command::new(&llvm_config)
        .arg(arg)
        .output()
        .unwrap_or_else(|error| panic!("`{} {arg}` failed: {error}", llvm_config.display()));

    String::from_utf8(output.stdout)
        .unwrap_or_else(|_| panic!("output of `{} {arg}` should be utf8", llvm_config.display()))
}

fn set_rustc_link_flags() {
    println!("cargo:rustc-link-search=native={}", llvm_config("--libdir"));

    for lib in [
        // These are required by ld.lld
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
        // The `llvm-sys` crate relies on `llvm-config` to obtain a list of required LLVM libraries
        // during the build process. This works well in typical native environments, where `llvm-config`
        // can accurately list the necessary libraries.
        // However, when cross-compiling to WebAssembly using Emscripten, `llvm-config` fails to recognize
        // JavaScript-based libraries, making it necessary to manually inject the required dependencies.
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
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "linux" {
        if target_env == "musl" {
            println!("cargo:rustc-link-lib=static=c++");
        } else {
            println!("cargo:rustc-link-lib=dylib=stdc++");
        }
    }
}

fn main() {
    println!(
        "cargo:rerun-if-env-changed={}",
        revive_llvm_builder::utils::REVIVE_LLVM_HOST_PREFIX
    );
    println!(
        "cargo:rerun-if-env-changed={}",
        revive_llvm_builder::utils::REVIVE_LLVM_TARGET_PREFIX
    );

    llvm_config("--cxxflags")
        .split_whitespace()
        .fold(&mut cc::Build::new(), |builder, flag| builder.flag(flag))
        .flag("-Wno-unused-parameter")
        .cpp(true)
        .file("src/linker.cpp")
        .compile("liblinker.a");

    set_rustc_link_flags();

    println!("cargo:rerun-if-changed=build.rs");
}
