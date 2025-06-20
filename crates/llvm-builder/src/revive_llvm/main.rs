//! The revive LLVM builder.

pub(crate) mod arguments;

use std::collections::HashSet;
use std::str::FromStr;

use anyhow::Context;
use clap::Parser;

use self::arguments::{Arguments, Subcommand};

fn main() {
    env_logger::init();

    match main_inner() {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            log::error!("{error:?}");
            std::process::exit(1)
        }
    }
}

fn main_inner() -> anyhow::Result<()> {
    let arguments = Arguments::parse();

    revive_llvm_builder::utils::directory_target_llvm(arguments.target_env);

    match arguments.subcommand {
        Subcommand::Build {
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
        } => {
            if arguments.target_env == revive_llvm_builder::TargetEnv::Emscripten {
                revive_llvm_builder::utils::install_emsdk()?;
            }

            revive_llvm_builder::init_submodule()?;

            let mut targets = targets
                .into_iter()
                .map(|target| revive_llvm_builder::Platform::from_str(target.as_str()))
                .collect::<Result<HashSet<revive_llvm_builder::Platform>, String>>()
                .map_err(|platform| anyhow::anyhow!("Unknown platform `{}`", platform))?;
            targets.insert(revive_llvm_builder::Platform::PolkaVM);

            log::info!("build targets: {:?}", &targets);

            let extra_args_unescaped: Vec<String> = extra_args
                .iter()
                .map(|argument| {
                    argument
                        .strip_prefix('\\')
                        .unwrap_or(argument.as_str())
                        .to_owned()
                })
                .collect();

            log::debug!("extra_args: {:#?}", extra_args);
            log::debug!("extra_args_unescaped: {:#?}", extra_args_unescaped);

            if let Some(ccache_variant) = ccache_variant {
                revive_llvm_builder::utils::check_presence(ccache_variant.to_string().as_str())?;
            }

            let mut projects = llvm_projects
                .into_iter()
                .map(|project| {
                    revive_llvm_builder::llvm_project::LLVMProject::from_str(
                        project.to_string().as_str(),
                    )
                })
                .collect::<Result<HashSet<revive_llvm_builder::llvm_project::LLVMProject>, String>>(
                )
                .map_err(|project| anyhow::anyhow!("Unknown LLVM project `{}`", project))?;
            projects.insert(revive_llvm_builder::llvm_project::LLVMProject::LLD);

            log::info!("build projects: {:?}", &projects);

            revive_llvm_builder::build(
                build_type,
                arguments.target_env,
                targets,
                projects,
                enable_rtti,
                default_target,
                enable_tests,
                enable_coverage,
                &extra_args_unescaped,
                ccache_variant,
                enable_assertions,
                sanitizer,
                enable_valgrind,
            )?;
        }

        Subcommand::Clean => {
            revive_llvm_builder::clean()
                .with_context(|| "Unable to remove target LLVM directory")?;
        }

        Subcommand::Builtins {
            build_type,
            default_target,
            extra_args,
            ccache_variant,
            sanitizer,
        } => {
            revive_llvm_builder::builtins::build(
                build_type,
                arguments.target_env,
                default_target,
                &extra_args,
                ccache_variant,
                sanitizer,
            )?;
        }
    }

    Ok(())
}
