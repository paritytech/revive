//! The revive LLVM amd64 `linux-gnu` builder.

use std::collections::HashSet;
use std::process::Command;

use crate::build_type::BuildType;
use crate::ccache_variant::CcacheVariant;
use crate::llvm_path::LLVMPath;
use crate::llvm_project::LLVMProject;
use crate::platforms::Platform;
use crate::sanitizer::Sanitizer;
use crate::target_triple::TargetTriple;

/// The building sequence.
#[allow(clippy::too_many_arguments)]
pub fn build(
    build_type: BuildType,
    targets: HashSet<Platform>,
    llvm_projects: HashSet<LLVMProject>,
    enable_rtti: bool,
    default_target: Option<TargetTriple>,
    enable_tests: bool,
    enable_coverage: bool,
    extra_args: Vec<String>,
    ccache_variant: Option<CcacheVariant>,
    enable_assertions: bool,
    sanitizer: Option<Sanitizer>,
    enable_valgrind: bool,
) -> anyhow::Result<()> {
    crate::utils::check_presence("cmake")?;
    crate::utils::check_presence("clang")?;
    crate::utils::check_presence("clang++")?;
    crate::utils::check_presence("lld")?;
    crate::utils::check_presence("ninja")?;

    let llvm_module_llvm = LLVMPath::llvm_module_llvm()?;
    let llvm_build_final = LLVMPath::llvm_build_final()?;
    let llvm_target_final = LLVMPath::llvm_target_final()?;

    crate::utils::command(
        Command::new("cmake")
            .args([
                "-S",
                llvm_module_llvm.to_string_lossy().as_ref(),
                "-B",
                llvm_build_final.to_string_lossy().as_ref(),
                "-G",
                "Ninja",
                format!(
                    "-DCMAKE_INSTALL_PREFIX='{}'",
                    llvm_target_final.to_string_lossy().as_ref(),
                )
                .as_str(),
                format!("-DCMAKE_BUILD_TYPE='{build_type}'").as_str(),
                "-DCMAKE_C_COMPILER='clang'",
                "-DCMAKE_CXX_COMPILER='clang++'",
                format!(
                    "-DLLVM_TARGETS_TO_BUILD='{}'",
                    targets
                        .into_iter()
                        .map(|platform| platform.to_string())
                        .collect::<Vec<String>>()
                        .join(";")
                )
                .as_str(),
                format!(
                    "-DLLVM_ENABLE_PROJECTS='{}'",
                    llvm_projects
                        .into_iter()
                        .map(|project| project.to_string())
                        .collect::<Vec<String>>()
                        .join(";")
                )
                .as_str(),
                "-DLLVM_USE_LINKER='lld'",
            ])
            .args(crate::platforms::shared::shared_build_opts_default_target(
                default_target,
            ))
            .args(crate::platforms::shared::shared_build_opts_tests(
                enable_tests,
            ))
            .args(crate::platforms::shared::shared_build_opts_coverage(
                enable_coverage,
            ))
            .args(crate::platforms::shared::shared_build_opts_ccache(
                ccache_variant,
            ))
            .args(crate::platforms::shared::SHARED_BUILD_OPTS)
            .args(crate::platforms::shared::SHARED_BUILD_OPTS_NOT_MUSL)
            .args(crate::platforms::shared::shared_build_opts_werror(
                crate::target_env::TargetEnv::GNU,
            ))
            .args(extra_args)
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
        "LLVM building cmake",
    )?;
    crate::utils::ninja(llvm_build_final.as_ref())?;
    Ok(())
}
