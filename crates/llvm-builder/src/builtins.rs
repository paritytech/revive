//! Utilities for compiling the LLVM compiler-rt builtins.

/// Static CFLAGS variable passed to the compiler building the compiler-rt builtins.
const C_FLAGS: [&str; 6] = [
    "--target=riscv64",
    "-march=rv64emac",
    "-mabi=lp64e",
    "-mcpu=generic-rv64",
    "-nostdlib",
    "-nodefaultlibs",
];

/// Static CMAKE arguments for building the compiler-rt builtins.
const CMAKE_STATIC_ARGS: [&str; 14] = [
    "-DCOMPILER_RT_BUILD_BUILTINS='On'",
    "-DCOMPILER_RT_BUILD_LIBFUZZER='Off'",
    "-DCOMPILER_RT_BUILD_MEMPROF='Off'",
    "-DCOMPILER_RT_BUILD_PROFILE='Off'",
    "-DCOMPILER_RT_BUILD_SANITIZERS='Off'",
    "-DCOMPILER_RT_BUILD_XRAY='Off'",
    "-DCOMPILER_RT_DEFAULT_TARGET_ONLY='On'",
    "-DCOMPILER_RT_BAREMETAL_BUILD='On'",
    "-DCMAKE_BUILD_WITH_INSTALL_RPATH=1",
    "-DCMAKE_EXPORT_COMPILE_COMMANDS='On'",
    "-DCMAKE_SYSTEM_NAME='unknown'",
    "-DCMAKE_C_COMPILER_TARGET='riscv64'",
    "-DCMAKE_ASM_COMPILER_TARGET='riscv64'",
    "-DCMAKE_CXX_COMPILER_TARGET='riscv64'",
];

/// Dynamic cmake arguments for building the compiler-rt builtins.
fn cmake_dynamic_args(build_type: crate::BuildType) -> anyhow::Result<[String; 11]> {
    let llvm_compiler_rt_target = crate::LLVMPath::llvm_target_compiler_rt()?;
    let llvm_target_final = crate::LLVMPath::llvm_target_final()?;

    let mut clang_path = llvm_target_final.to_path_buf();
    clang_path.push("bin/clang");

    let mut llvm_config_path = llvm_target_final.to_path_buf();
    llvm_config_path.push("bin/llvm-config");

    let mut ar_path = llvm_target_final.to_path_buf();
    ar_path.push("bin/llvm-ar");

    let mut nm_path = llvm_target_final.to_path_buf();
    nm_path.push("bin/llvm-nm");

    let mut ranlib_path = llvm_target_final.to_path_buf();
    ranlib_path.push("bin/llvm-ranlib");

    Ok([
        format!(
            "-DCMAKE_INSTALL_PREFIX='{}'",
            llvm_compiler_rt_target.to_string_lossy().as_ref(),
        ),
        format!("-DCMAKE_BUILD_TYPE='{build_type}'"),
        format!(
            "-DCOMPILER_RT_TEST_COMPILER='{}'",
            clang_path.to_string_lossy()
        ),
        format!("-DCMAKE_C_FLAGS='{}'", C_FLAGS.join(" ")),
        format!("-DCMAKE_ASM_FLAGS='{}'", C_FLAGS.join(" ")),
        format!("-DCMAKE_CXX_FLAGS='{}'", C_FLAGS.join(" ")),
        format!("-DCMAKE_C_COMPILER='{}'", clang_path.to_string_lossy()),
        format!("-DCMAKE_AR='{}'", ar_path.to_string_lossy()),
        format!("-DCMAKE_NM='{}'", nm_path.to_string_lossy()),
        format!("-DCMAKE_RANLIB='{}'", ranlib_path.to_string_lossy()),
        format!(
            "-DLLVM_CONFIG_PATH='{}'",
            llvm_config_path.to_string_lossy()
        ),
    ])
}

/// Build the compiler-rt builtins library.
pub fn build(
    build_type: crate::BuildType,
    default_target: Option<crate::TargetTriple>,
    extra_args: Vec<String>,
    ccache_variant: Option<crate::ccache_variant::CcacheVariant>,
    sanitizer: Option<crate::sanitizer::Sanitizer>,
) -> anyhow::Result<()> {
    log::info!("building compiler-rt for rv64emac");

    crate::utils::check_presence("cmake")?;
    crate::utils::check_presence("ninja")?;

    let llvm_module_compiler_rt = crate::LLVMPath::llvm_module_compiler_rt()?;
    let llvm_compiler_rt_build = crate::LLVMPath::llvm_build_compiler_rt()?;

    crate::utils::command(
        std::process::Command::new("cmake")
            .args([
                "-S",
                llvm_module_compiler_rt.to_string_lossy().as_ref(),
                "-B",
                llvm_compiler_rt_build.to_string_lossy().as_ref(),
                "-G",
                "Ninja",
            ])
            .args(CMAKE_STATIC_ARGS)
            .args(cmake_dynamic_args(build_type)?)
            .args(extra_args)
            .args(crate::platforms::shared::shared_build_opts_ccache(
                ccache_variant,
            ))
            .args(crate::platforms::shared::shared_build_opts_default_target(
                default_target,
            ))
            .args(crate::platforms::shared::shared_build_opts_sanitizers(
                sanitizer,
            )),
        "LLVM compiler-rt building cmake",
    )?;

    crate::utils::ninja(&llvm_compiler_rt_build)?;

    Ok(())
}
