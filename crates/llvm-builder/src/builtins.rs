//! Utilities for compiling the LLVM compiler-rt builtins.

use crate::utils::path_windows_to_unix as to_unix;
use std::{env::consts::EXE_EXTENSION, process::Command};

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
fn cmake_dynamic_args(
    build_type: crate::BuildType,
    target_env: crate::target_env::TargetEnv,
) -> anyhow::Result<[String; 13]> {
    let llvm_compiler_rt_target = crate::LLVMPath::llvm_target_compiler_rt()?;

    // The Emscripten target needs to use the host LLVM tools.
    let llvm_target_host = if target_env == crate::target_env::TargetEnv::Emscripten {
        crate::LLVMPath::llvm_build_host()?
    } else {
        crate::LLVMPath::llvm_target_final()?
    };

    let mut clang_path = llvm_target_host.to_path_buf();
    clang_path.push("bin/clang");
    clang_path.set_extension(EXE_EXTENSION);

    let mut clangxx_path = llvm_target_host.to_path_buf();
    clangxx_path.push("bin/clang++");
    clangxx_path.set_extension(EXE_EXTENSION);

    let mut llvm_config_path = llvm_target_host.to_path_buf();
    llvm_config_path.push("bin/llvm-config");
    llvm_config_path.set_extension(EXE_EXTENSION);

    let mut ar_path = llvm_target_host.to_path_buf();
    ar_path.push("bin/llvm-ar");
    ar_path.set_extension(EXE_EXTENSION);

    let mut nm_path = llvm_target_host.to_path_buf();
    nm_path.push("bin/llvm-nm");
    nm_path.set_extension(EXE_EXTENSION);

    let mut ranlib_path = llvm_target_host.to_path_buf();
    ranlib_path.push("bin/llvm-ranlib");
    ranlib_path.set_extension(EXE_EXTENSION);

    let mut linker_path = llvm_target_host.to_path_buf();
    linker_path.push("bin/ld.lld");
    linker_path.set_extension(EXE_EXTENSION);

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
        format!(
            "-DCMAKE_C_COMPILER='{}'",
            to_unix(clang_path.clone())?.display()
        ),
        format!("-DCMAKE_ASM_COMPILER='{}'", to_unix(clang_path)?.display()),
        format!(
            "-DCMAKE_CXX_COMPILER='{}'",
            to_unix(clangxx_path)?.display()
        ),
        format!("-DCMAKE_AR='{}'", to_unix(ar_path)?.display()),
        format!("-DCMAKE_NM='{}'", to_unix(nm_path)?.display()),
        format!("-DCMAKE_RANLIB='{}'", to_unix(ranlib_path)?.display()),
        format!(
            "-DLLVM_CONFIG_PATH='{}'",
            llvm_config_path.to_string_lossy()
        ),
    ])
}

/// Build the compiler-rt builtins library.
pub fn build(
    build_type: crate::BuildType,
    target_env: crate::target_env::TargetEnv,
    default_target: Option<crate::TargetTriple>,
    extra_args: &[String],
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
            .args(cmake_dynamic_args(build_type, target_env)?)
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

    crate::utils::command(
        Command::new("cmake").args([
            "--build",
            llvm_compiler_rt_build.to_string_lossy().as_ref(),
            "--target",
            "install",
            "--config",
            build_type.to_string().as_str(),
        ]),
        "Building",
    )?;

    Ok(())
}
