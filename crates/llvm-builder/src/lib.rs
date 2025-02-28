//! The revive LLVM builder library.

pub mod build_type;
pub mod builtins;
pub mod ccache_variant;
pub mod llvm_path;
pub mod llvm_project;
pub mod lock;
pub mod platforms;
pub mod sanitizer;
pub mod target_env;
pub mod target_triple;
pub mod utils;

pub use self::build_type::BuildType;
pub use self::llvm_path::LLVMPath;
pub use self::lock::Lock;
pub use self::platforms::Platform;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
pub use target_env::TargetEnv;
pub use target_triple::TargetTriple;

/// Executes the LLVM repository cloning.
pub fn clone(lock: Lock, deep: bool, target_env: TargetEnv) -> anyhow::Result<()> {
    utils::check_presence("git")?;

    if target_env == TargetEnv::Emscripten {
        utils::install_emsdk()?;
    }

    let destination_path = PathBuf::from(LLVMPath::DIRECTORY_LLVM_SOURCE);
    if destination_path.exists() {
        log::warn!(
            "LLVM repository directory {} already exists, falling back to checkout",
            destination_path.display()
        );
        return checkout(lock, false);
    }

    let mut clone_args = vec!["clone", "--branch", lock.branch.as_str()];
    if !deep {
        clone_args.push("--depth");
        clone_args.push("1");
    }

    utils::command(
        Command::new("git")
            .args(clone_args)
            .arg(lock.url.as_str())
            .arg(destination_path.to_string_lossy().as_ref()),
        "LLVM repository cloning",
    )?;

    if let Some(r#ref) = lock.r#ref {
        utils::command(
            Command::new("git")
                .args(["checkout", r#ref.as_str()])
                .current_dir(destination_path.to_string_lossy().as_ref()),
            "LLVM repository commit checking out",
        )?;
    }

    Ok(())
}

/// Executes the checkout of the specified branch.
pub fn checkout(lock: Lock, force: bool) -> anyhow::Result<()> {
    let destination_path = PathBuf::from(LLVMPath::DIRECTORY_LLVM_SOURCE);

    utils::command(
        Command::new("git")
            .current_dir(destination_path.as_path())
            .args(["fetch", "--all", "--tags"]),
        "LLVM repository data fetching",
    )?;

    if force {
        utils::command(
            Command::new("git")
                .current_dir(destination_path.as_path())
                .args(["clean", "-d", "-x", "--force"]),
            "LLVM repository cleaning",
        )?;
    }

    utils::command(
        Command::new("git")
            .current_dir(destination_path.as_path())
            .args(["checkout", "--force", lock.branch.as_str()]),
        "LLVM repository data pulling",
    )?;

    if let Some(r#ref) = lock.r#ref {
        let mut checkout_command = Command::new("git");
        checkout_command.current_dir(destination_path.as_path());
        checkout_command.arg("checkout");
        if force {
            checkout_command.arg("--force");
        }
        checkout_command.arg(r#ref);
        utils::command(&mut checkout_command, "LLVM repository checking out")?;
    }

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
    remove_if_exists(&PathBuf::from(LLVMPath::DIRECTORY_LLVM_SOURCE))?;
    remove_if_exists(&PathBuf::from(LLVMPath::DIRECTORY_LLVM_HOST_SOURCE))?;

    Ok(())
}
