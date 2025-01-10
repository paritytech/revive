//! The revive LLVM `wasm32_unknown_emscripten` builder.
//!
//! Cross-compiling LLVM for Emscripten requires llvm-tblgen, clang-tblgen and llvm-config.

use std::{collections::HashSet, path::Path, process::Command};

/// The building sequence.
#[allow(clippy::too_many_arguments)]
pub fn build(
    build_type: crate::BuildType,
    targets: HashSet<crate::Platform>,
    llvm_projects: HashSet<crate::llvm_project::LLVMProject>,
    enable_rtti: bool,
    default_target: Option<crate::TargetTriple>,
    enable_tests: bool,
    enable_coverage: bool,
    extra_args: &[String],
    ccache_variant: Option<crate::ccache_variant::CcacheVariant>,
    enable_assertions: bool,
    sanitizer: Option<crate::sanitizer::Sanitizer>,
    enable_valgrind: bool,
) -> anyhow::Result<()> {
    crate::utils::check_presence("cmake")?;
    crate::utils::check_presence("ninja")?;
    crate::utils::check_presence("emsdk")?;
    crate::utils::check_presence("clang")?;
    crate::utils::check_presence("clang++")?;
    if cfg!(target_os = "linux") {
        crate::utils::check_presence("lld")?;
    }

    let llvm_module_llvm = crate::LLVMPath::llvm_module_llvm()?;
    let llvm_host_module_llvm = crate::LLVMPath::llvm_host_module_llvm()?;

    let llvm_build_host = crate::LLVMPath::llvm_build_host()?;
    let llvm_target_host = crate::LLVMPath::llvm_target_host()?;

    let llvm_build_final = crate::LLVMPath::llvm_build_final()?;
    let llvm_target_final = crate::LLVMPath::llvm_target_final()?;

    build_host(
        llvm_host_module_llvm.as_path(),
        llvm_build_host.as_path(),
        llvm_target_host.as_path(),
        ccache_variant,
    )?;

    build_target(
        build_type,
        targets,
        llvm_projects,
        enable_rtti,
        default_target,
        llvm_module_llvm.as_path(),
        llvm_build_final.as_path(),
        llvm_target_final.as_path(),
        llvm_build_host.as_path(),
        enable_tests,
        enable_coverage,
        extra_args,
        ccache_variant,
        enable_assertions,
        sanitizer,
        enable_valgrind,
    )?;

    let mut source_path = llvm_build_host.clone();
    source_path.push("bin/llvm-config");

    let mut destination_path = llvm_target_final.clone();
    destination_path.push("bin/llvm-config");

    fs_extra::file::copy(
        source_path,
        destination_path,
        &fs_extra::file::CopyOptions {
            overwrite: true,
            ..Default::default()
        },
    )?;

    Ok(())
}

/// The host toolchain building sequence.
fn build_host(
    source_directory: &Path,
    build_directory: &Path,
    target_directory: &Path,
    ccache_variant: Option<crate::ccache_variant::CcacheVariant>,
) -> anyhow::Result<()> {
    log::info!("building the LLVM Emscripten host utilities");

    crate::utils::command(
        Command::new("cmake")
            .args([
                "-S",
                source_directory.to_string_lossy().as_ref(),
                "-B",
                build_directory.to_string_lossy().as_ref(),
                "-G",
                "Ninja",
                "-DLINKER_SUPPORTS_COLOR_DIAGNOSTICS=0",
                format!(
                    "-DCMAKE_INSTALL_PREFIX='{}'",
                    target_directory.to_string_lossy()
                )
                .as_str(),
                "-DLLVM_BUILD_SHARED_LIBS='Off'",
                "-DCMAKE_BUILD_TYPE='Release'",
                "-DLLVM_TARGETS_TO_BUILD='WebAssembly'",
                "-DLLVM_ENABLE_PROJECTS='clang'",
            ])
            .args(crate::platforms::shared::SHARED_BUILD_OPTS)
            .args(crate::platforms::shared::SHARED_BUILD_OPTS_NOT_MUSL)
            .args(crate::platforms::shared::shared_build_opts_ccache(
                ccache_variant,
            )),
        "LLVM host building cmake config",
    )?;

    crate::utils::command(
        Command::new("cmake")
            .arg("--build")
            .arg(build_directory)
            .arg("--")
            .arg("llvm-tblgen")
            .arg("clang-tblgen")
            .arg("llvm-config"),
        "LLVM Emscripten host utilities build",
    )?;

    Ok(())
}

/// The target toolchain building sequence.
#[allow(clippy::too_many_arguments)]
fn build_target(
    build_type: crate::BuildType,
    targets: HashSet<crate::Platform>,
    llvm_projects: HashSet<crate::llvm_project::LLVMProject>,
    enable_rtti: bool,
    default_target: Option<crate::TargetTriple>,
    source_directory: &Path,
    build_directory: &Path,
    target_directory: &Path,
    host_build_directory: &Path,
    enable_tests: bool,
    enable_coverage: bool,
    extra_args: &[String],
    ccache_variant: Option<crate::ccache_variant::CcacheVariant>,
    enable_assertions: bool,
    sanitizer: Option<crate::sanitizer::Sanitizer>,
    enable_valgrind: bool,
) -> anyhow::Result<()> {
    let mut llvm_tblgen_path = host_build_directory.to_path_buf();
    llvm_tblgen_path.push("bin/llvm-tblgen");

    let mut clang_tblgen_path = host_build_directory.to_path_buf();
    clang_tblgen_path.push("bin/clang-tblgen");

    crate::utils::command(
        Command::new("emcmake")
            .env("EMCC_DEBUG", "2")
            .env("CXXFLAGS", "-Dwait4=__syscall_wait4")
            .env("LDFLAGS", "-lnodefs.js -s NO_INVOKE_RUN -s EXIT_RUNTIME -s INITIAL_MEMORY=64MB -s ALLOW_MEMORY_GROWTH -s EXPORTED_RUNTIME_METHODS=FS,callMain,NODEFS -s MODULARIZE -s EXPORT_ES6 -s WASM_BIGINT")
            .arg("cmake")
            .args([
                "-S",
                source_directory.to_string_lossy().as_ref(),
                "-B",
                build_directory.to_string_lossy().as_ref(),
                "-G",
                "Ninja",
                "-DLINKER_SUPPORTS_COLOR_DIAGNOSTICS=0",
                "-DCMAKE_BUILD_WITH_INSTALL_RPATH=1",
                // Enable thin LTO but emscripten has various issues with it.
                // FIXME: https://github.com/paritytech/revive/issues/148
                //"-DLLVM_ENABLE_LTO='Thin'",
                //"-DCMAKE_EXE_LINKER_FLAGS='-Wl,-u,htons -Wl,-u,htonl -Wl,-u,fileno -Wl,-u,ntohs'",
                &format!(
                    "-DCMAKE_INSTALL_PREFIX='{}'",
                    target_directory.to_string_lossy()
                ),
                &format!("-DCMAKE_BUILD_TYPE='{build_type}'"),
                &format!(
                    "-DLLVM_TARGETS_TO_BUILD='{}'",
                    targets
                        .into_iter()
                        .map(|platform| platform.to_string())
                        .collect::<Vec<String>>()
                        .join(";")
                ),
                &format!(
                    "-DLLVM_ENABLE_PROJECTS='{}'",
                    llvm_projects
                        .into_iter()
                        .map(|project| project.to_string())
                        .collect::<Vec<String>>()
                        .join(";")
                ),
                "-DLLVM_BUILD_SHARED_LIBS='Off'",
                "-DLLVM_ENABLE_DUMP='Off'",
                "-DLLVM_ENABLE_EXPENSIVE_CHECKS='Off'",
                "-DLLVM_ENABLE_BACKTRACES='Off'",
                "-DLLVM_ENABLE_BACKTRACES='Off'",
                "-DLLVM_ENABLE_THREADS='Off'",
                "-DLLVM_BUILD_TOOLS='Off'",
                &format!("-DLLVM_TABLEGEN='{}'", llvm_tblgen_path.to_string_lossy()),
                &format!("-DCLANG_TABLEGEN='{}'", clang_tblgen_path.to_string_lossy()),
            ])
            .args(crate::platforms::shared::shared_build_opts_default_target(
                default_target,
            ))
            .args(crate::platforms::shared::SHARED_BUILD_OPTS)
            .args(crate::platforms::shared::SHARED_BUILD_OPTS_NOT_MUSL)
            .args(crate::platforms::shared::shared_build_opts_werror(crate::target_env::TargetEnv::Emscripten))
            .args(crate::platforms::shared::shared_build_opts_tests(
                enable_tests,
            ))
            .args(crate::platforms::shared::shared_build_opts_coverage(
                enable_coverage,
            ))
            .args(extra_args)
            .args(crate::platforms::shared::shared_build_opts_ccache(
                ccache_variant,
            ))
            .args(crate::platforms::shared::shared_build_opts_assertions(
                enable_assertions,
            ))
            .args(crate::platforms::shared::shared_build_opts_rtti(
                enable_rtti,
            ))
            .args(crate::platforms::shared::shared_build_opts_sanitizers(
                sanitizer,
            ))
            .args(crate::platforms::shared::shared_build_opts_valgrind(
                enable_valgrind,
            )),
        "LLVM target building cmake",
    )?;

    crate::utils::ninja(build_directory)?;

    Ok(())
}
