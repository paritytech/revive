//! The revive LLVM builder library.

pub mod build_type;
pub mod builtins;
pub mod ccache_variant;
pub mod llvm_path;
pub mod llvm_project;
pub mod platforms;
pub mod sanitizer;
pub mod target_env;
pub mod target_triple;
pub mod utils;

pub use self::build_type::BuildType;
pub use self::llvm_path::LLVMPath;
pub use self::platforms::Platform;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
pub use target_env::TargetEnv;
pub use target_triple::TargetTriple;

/// Initializes the LLVM submodule if not already done.
pub fn init_submodule() -> anyhow::Result<()> {
    utils::check_presence("git")?;

    let destination_path = PathBuf::from(LLVMPath::DIRECTORY_LLVM_SOURCE);
    if destination_path.join(".git").exists() {
        log::info!("LLVM submodule already initialized");
        return Ok(());
    }

    utils::command(
        Command::new("git").args(["submodule", "update", "--init", "--recursive"]),
        "LLVM submodule initialization",
    )?;

    Ok(())
}

/// Executes the building of the LLVM framework for the platform determined by the cfg macro.
/// Since cfg is evaluated at compile time, overriding the platform with a command-line
/// argument is not possible. So for cross-platform testing, comment out all but the
/// line to be tested, and perhaps also checks in the platform-specific build method.
#[allow(clippy::too_many_arguments)]
pub fn build(
    build_type: BuildType,
    target_env: TargetEnv,
    targets: HashSet<Platform>,
    llvm_projects: HashSet<llvm_project::LLVMProject>,
    enable_rtti: bool,
    default_target: Option<TargetTriple>,
    enable_tests: bool,
    enable_coverage: bool,
    extra_args: &[String],
    ccache_variant: Option<ccache_variant::CcacheVariant>,
    enable_assertions: bool,
    sanitizer: Option<sanitizer::Sanitizer>,
    enable_valgrind: bool,
) -> anyhow::Result<()> {
    log::trace!("build type: {:?}", build_type);
    log::trace!("target env: {:?}", target_env);
    log::trace!("targets: {:?}", targets);
    log::trace!("llvm projects: {:?}", llvm_projects);
    log::trace!("enable rtti: {:?}", enable_rtti);
    log::trace!("default target: {:?}", default_target);
    log::trace!("eneable tests: {:?}", enable_tests);
    log::trace!("enable_coverage: {:?}", enable_coverage);
    log::trace!("extra args: {:?}", extra_args);
    log::trace!("sanitzer: {:?}", sanitizer);
    log::trace!("enable valgrind: {:?}", enable_valgrind);

    if !PathBuf::from(LLVMPath::DIRECTORY_LLVM_SOURCE).exists() {
        log::error!(
            "LLVM project source directory {} does not exist (run `revive-llvm --target-env {} clone`)",
            LLVMPath::DIRECTORY_LLVM_SOURCE,
            target_env
        )
    }

    std::fs::create_dir_all(llvm_path::DIRECTORY_LLVM_TARGET.get().unwrap())?;

    if cfg!(target_arch = "x86_64") {
        if cfg!(target_os = "linux") {
            if target_env == TargetEnv::MUSL {
                platforms::x86_64_linux_musl::build(
                    build_type,
                    targets,
                    llvm_projects,
                    enable_rtti,
                    default_target,
                    enable_tests,
                    enable_coverage,
                    extra_args,
                    ccache_variant,
                    enable_assertions,
                    sanitizer,
                    enable_valgrind,
                )?;
            } else if target_env == TargetEnv::GNU {
                platforms::x86_64_linux_gnu::build(
                    build_type,
                    targets,
                    llvm_projects,
                    enable_rtti,
                    default_target,
                    enable_tests,
                    enable_coverage,
                    extra_args,
                    ccache_variant,
                    enable_assertions,
                    sanitizer,
                    enable_valgrind,
                )?;
            } else if target_env == TargetEnv::Emscripten {
                platforms::wasm32_emscripten::build(
                    build_type,
                    targets,
                    llvm_projects,
                    enable_rtti,
                    default_target,
                    enable_tests,
                    enable_coverage,
                    extra_args,
                    ccache_variant,
                    enable_assertions,
                    sanitizer,
                    enable_valgrind,
                )?;
            } else {
                anyhow::bail!("Unsupported target environment for x86_64 and Linux");
            }
        } else if cfg!(target_os = "macos") {
            platforms::x86_64_macos::build(
                build_type,
                targets,
                llvm_projects,
                enable_rtti,
                default_target,
                enable_tests,
                enable_coverage,
                extra_args,
                ccache_variant,
                enable_assertions,
                sanitizer,
            )?;
        } else if cfg!(target_os = "windows") {
            platforms::x86_64_windows_msvc::build(
                build_type,
                targets,
                llvm_projects,
                enable_rtti,
                default_target,
                enable_tests,
                enable_coverage,
                extra_args,
                ccache_variant,
                enable_assertions,
                sanitizer,
            )?;
        } else {
            anyhow::bail!("Unsupported target OS for x86_64");
        }
    } else if cfg!(target_arch = "aarch64") {
        if cfg!(target_os = "linux") {
            if target_env == TargetEnv::MUSL {
                platforms::aarch64_linux_musl::build(
                    build_type,
                    targets,
                    llvm_projects,
                    enable_rtti,
                    default_target,
                    enable_tests,
                    enable_coverage,
                    extra_args,
                    ccache_variant,
                    enable_assertions,
                    sanitizer,
                    enable_valgrind,
                )?;
            } else if target_env == TargetEnv::GNU {
                platforms::aarch64_linux_gnu::build(
                    build_type,
                    targets,
                    llvm_projects,
                    enable_rtti,
                    default_target,
                    enable_tests,
                    enable_coverage,
                    extra_args,
                    ccache_variant,
                    enable_assertions,
                    sanitizer,
                    enable_valgrind,
                )?;
            } else {
                anyhow::bail!("Unsupported target environment for aarch64 and Linux");
            }
        } else if cfg!(target_os = "macos") {
            if target_env == TargetEnv::Emscripten {
                platforms::wasm32_emscripten::build(
                    build_type,
                    targets,
                    llvm_projects,
                    enable_rtti,
                    default_target,
                    enable_tests,
                    enable_coverage,
                    extra_args,
                    ccache_variant,
                    enable_assertions,
                    sanitizer,
                    enable_valgrind,
                )?;
            } else {
                platforms::aarch64_macos::build(
                    build_type,
                    targets,
                    llvm_projects,
                    enable_rtti,
                    default_target,
                    enable_tests,
                    enable_coverage,
                    extra_args,
                    ccache_variant,
                    enable_assertions,
                    sanitizer,
                )?;
            }
        } else {
            anyhow::bail!("Unsupported target OS for aarch64");
        }
    } else {
        anyhow::bail!("Unsupported target architecture");
    }

    crate::builtins::build(
        build_type,
        target_env,
        default_target,
        extra_args,
        ccache_variant,
        sanitizer,
    )?;

    Ok(())
}

/// Executes the build artifacts cleaning.
pub fn clean() -> anyhow::Result<()> {
    let remove_if_exists = |path: &Path| {
        if !path.exists() {
            return Ok(());
        }
        log::info!("deleting {}", path.display());
        std::fs::remove_dir_all(path)
    };

    remove_if_exists(
        llvm_path::DIRECTORY_LLVM_TARGET
            .get()
            .expect("target_env is always set because of the default value")
            .parent()
            .expect("target_env parent directory is target-llvm"),
    )?;
    remove_if_exists(&PathBuf::from(LLVMPath::DIRECTORY_EMSDK_SOURCE))?;

    Ok(())
}
