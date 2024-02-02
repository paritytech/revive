fn llvm_config(arg: &str) -> String {
    let output = std::process::Command::new("llvm-config")
        .args([arg])
        .output()
        .unwrap_or_else(|_| panic!("`llvm-config {arg}` failed"));

    String::from_utf8(output.stdout)
        .unwrap_or_else(|_| panic!("output of `llvm-config {arg}` should be utf8"))
}

fn main() {
    let mut builder = cc::Build::new();
    llvm_config("--cxxflags")
        .split_whitespace()
        .fold(&mut builder, |builder, flag| builder.flag(flag))
        .cpp(true)
        .file("src/linker.cpp")
        .compile("liblinker.a");

    println!("cargo:rustc-link-search=native={}", llvm_config("--libdir"));

    for lib in ["lldELF", "lldCommon", "lldMachO"] {
        println!("cargo:rustc-link-lib=static={lib}");
    }

    println!("cargo:rerun-if-changed=build.rs");
}
