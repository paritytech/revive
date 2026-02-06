fn set_rustc_link_flags() {
    let llvm_lib_path = match std::env::var(revive_build_utils::REVIVE_LLVM_TARGET_PREFIX) {
        Ok(path) => std::path::PathBuf::from(path).join("lib"),
        _ => revive_build_utils::llvm_lib_dir(),
    };

    println!(
        "cargo:rustc-link-search=native={}",
        llvm_lib_path.to_string_lossy()
    );

    for lib in [
        // These are required by ld.lld
        "lldELF",
        "lldCommon",
        "lldMachO",
        "lldWasm",
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
        "LLVMTextAPI",
        "LLVMDebugInfoDWARFLowLevel",
        "LLVMDebugInfoGSYM",
        "LLVMDebugInfoMSF",
        "LLVMDebugInfoPDB",
        "LLVMDebugInfoBTF",
        "LLVMInterfaceStub",
        "LLVMCGData",
        "LLVMMIRParser",
        "LLVMDWARFLinker",
        "LLVMDWARFLinkerParallel",
        "LLVMDWARFLinkerClassic",
        "LLVMLibDriver",
        "LLVMDlltoolDriver",
        "LLVMTextAPIBinaryReader",
        "LLVMCoverage",
        "LLVMLineEditor",
        "LLVMRISCVTargetMCA",
        "LLVMRuntimeDyld",
        "LLVMDWP",
        "LLVMDWARFCFIChecker",
        "LLVMDebugInfoLogicalView",
        "LLVMMCA",
        "LLVMipo",
        "LLVMVectorize",
        "LLVMSandboxIR",
        "LLVMExtensions",
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
        revive_build_utils::REVIVE_LLVM_HOST_PREFIX
    );
    println!(
        "cargo:rerun-if-env-changed={}",
        revive_build_utils::REVIVE_LLVM_TARGET_PREFIX
    );

    revive_build_utils::llvm_cxx_flags()
        .split_whitespace()
        .fold(&mut cc::Build::new(), |builder, flag| builder.flag(flag))
        .warnings(false)
        .cpp(true)
        .file("src/linker.cpp")
        .compile("liblinker.a");

    set_rustc_link_flags();

    println!("cargo:rerun-if-changed=build.rs");
}
