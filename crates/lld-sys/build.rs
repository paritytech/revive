fn llvm_config(arg: &str) -> String {
    let output = std::process::Command::new("llvm-config")
        .args([arg])
        .output()
        .unwrap_or_else(|_| panic!("`llvm-config {arg}` failed"));

    String::from_utf8(output.stdout)
        .unwrap_or_else(|_| panic!("output of `llvm-config {arg}` should be utf8"))
}

fn set_rustc_link_flags() {
    println!("cargo:rustc-link-search=native={}", llvm_config("--libdir"));

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
    ] {
        println!("cargo:rustc-link-lib=static={lib}");
    }

    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-lib=dylib=stdc++");
}

fn main() {
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
