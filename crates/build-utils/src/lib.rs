//! The compiler build utilities library.

/// The revive LLVM host dependency directory prefix environment variable.
pub const REVIVE_LLVM_HOST_PREFIX: &str = "LLVM_SYS_181_PREFIX";

/// The revive LLVM target dependency directory prefix environment variable.
pub const REVIVE_LLVM_TARGET_PREFIX: &str = "REVIVE_LLVM_TARGET_PREFIX";

/// The revive LLVM host tool help link.
pub const REVIVE_LLVM_BUILDER_HELP_LINK: &str =
    "https://github.com/paritytech/revive?tab=readme-ov-file#building-from-source";

/// Constructs a path to the LLVM tool `name`.
///
/// Respects the [`REVIVE_LLVM_HOST_PREFIX`] environment variable.
pub fn llvm_host_tool(name: &str) -> std::path::PathBuf {
    std::env::var_os(REVIVE_LLVM_HOST_PREFIX)
        .map(Into::<std::path::PathBuf>::into)
        .unwrap_or_else(|| {
            panic!("install LLVM using the revive-llvm builder and export '{REVIVE_LLVM_HOST_PREFIX}'; see also: {REVIVE_LLVM_BUILDER_HELP_LINK}")
        })
        .join("bin")
        .join(name)
}

/// Returns the LLVM lib dir.
///
/// Respects the [`REVIVE_LLVM_HOST_PREFIX`] environment variable.
pub fn llvm_lib_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(llvm_config("--libdir").trim())
}

/// Returns the LLVM CXX compiler flags.
///
/// Respects the [`REVIVE_LLVM_HOST_PREFIX`] environment variable.
pub fn llvm_cxx_flags() -> String {
    llvm_config("--cxxflags")
}

/// Execute the `llvm-config` utility respecting the [`REVIVE_LLVM_HOST_PREFIX`] environment variable.
fn llvm_config(arg: &str) -> String {
    let llvm_config = llvm_host_tool("llvm-config");
    let output = std::process::Command::new(&llvm_config)
        .arg(arg)
        .output()
        .unwrap_or_else(|error| panic!("`{} {arg}` failed: {error}", llvm_config.display()));

    String::from_utf8(output.stdout)
        .unwrap_or_else(|_| panic!("output of `{} {arg}` should be utf8", llvm_config.display()))
}
