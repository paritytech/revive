//! Solidity to PolkaVM compiler library.

#![allow(clippy::too_many_arguments)]

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;

#[cfg(feature = "parallel")]
use rayon::iter::IntoParallelIterator;
#[cfg(feature = "parallel")]
use rayon::iter::ParallelIterator;
use revive_common::MetadataHash;
use revive_llvm_context::OptimizerSettings;
use revive_solc_json_interface::CombinedJsonSelector;
use revive_solc_json_interface::ResolcWarning;
use revive_solc_json_interface::SolcStandardJsonInput;
use revive_solc_json_interface::SolcStandardJsonInputLanguage;
use revive_solc_json_interface::SolcStandardJsonInputSettingsLibraries;
use revive_solc_json_interface::SolcStandardJsonInputSettingsOptimizer;
use revive_solc_json_interface::SolcStandardJsonInputSettingsPolkaVM;
use revive_solc_json_interface::SolcStandardJsonInputSettingsPolkaVMMemory;
use revive_solc_json_interface::SolcStandardJsonInputSettingsSelection;
use revive_solc_json_interface::SolcStandardJsonOutputError;
use revive_solc_json_interface::SolcStandardJsonOutputErrorHandler;

use crate::linker::Linker;

pub use self::build::contract::Contract as ContractBuild;
pub use self::build::Build;
pub use self::missing_libraries::MissingLibraries;
pub use self::process::input::Input as ProcessInput;
#[cfg(not(target_os = "emscripten"))]
pub use self::process::native_process::NativeProcess;
pub use self::process::output::Output as ProcessOutput;
#[cfg(target_os = "emscripten")]
pub use self::process::worker_process::WorkerProcess;
pub use self::process::Process;
pub use self::project::contract::Contract as ProjectContract;
pub use self::project::Project;
pub use self::r#const::*;
#[cfg(not(target_os = "emscripten"))]
pub use self::solc::solc_compiler::SolcCompiler;
#[cfg(target_os = "emscripten")]
pub use self::solc::soljson_compiler::SoljsonCompiler;
pub use self::solc::version::Version as SolcVersion;
pub use self::solc::Compiler;
pub use self::solc::FIRST_SUPPORTED_VERSION as SolcFirstSupportedVersion;
pub use self::solc::LAST_SUPPORTED_VERSION as SolcLastSupportedVersion;
pub use self::version::Version as ResolcVersion;

pub(crate) mod build;
pub(crate) mod r#const;
pub(crate) mod linker;
pub(crate) mod missing_libraries;
pub(crate) mod process;
pub(crate) mod project;
pub(crate) mod solc;
#[cfg(not(target_os = "emscripten"))]
pub mod test_utils;
pub mod tests;
pub(crate) mod version;

/// The rayon worker stack size.
pub const RAYON_WORKER_STACK_SIZE: usize = 64 * 1024 * 1024;

/// Runs the Yul mode.
pub fn yul<T: Compiler>(
    solc: &T,
    input_files: &[PathBuf],
    libraries: &[String],
    metadata_hash: MetadataHash,
    messages: &mut Vec<SolcStandardJsonOutputError>,
    optimizer_settings: revive_llvm_context::OptimizerSettings,
    debug_config: revive_llvm_context::DebugConfig,
    llvm_arguments: &[String],
    memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
) -> anyhow::Result<Build> {
    let libraries = SolcStandardJsonInputSettingsLibraries::try_from(libraries)?;
    solc.validate_yul_paths(input_files, libraries.clone(), messages)?;

    let linker_symbols = libraries.as_linker_symbols()?;
    let project = Project::try_from_yul_paths(input_files, None, libraries, &debug_config)?;
    let mut build = project.compile(
        messages,
        optimizer_settings,
        metadata_hash,
        &debug_config,
        llvm_arguments,
        memory_config,
    )?;
    build.take_and_write_warnings();
    build.check_errors()?;

    let mut build = build.link(linker_symbols, &debug_config);
    build.take_and_write_warnings();
    build.check_errors()?;
    Ok(build)
}

/// Runs the standard output mode.
pub fn standard_output<T: Compiler>(
    solc: &T,
    input_files: &[PathBuf],
    libraries: &[String],
    metadata_hash: MetadataHash,
    messages: &mut Vec<SolcStandardJsonOutputError>,
    evm_version: Option<revive_common::EVMVersion>,
    solc_optimizer_enabled: bool,
    optimizer_settings: revive_llvm_context::OptimizerSettings,
    base_path: Option<String>,
    include_paths: Vec<String>,
    allow_paths: Option<String>,
    remappings: BTreeSet<String>,
    suppressed_warnings: Vec<ResolcWarning>,
    debug_config: revive_llvm_context::DebugConfig,
    llvm_arguments: Vec<String>,
    memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
) -> anyhow::Result<Build> {
    let solc_version = solc.version()?;
    let mut solc_input = SolcStandardJsonInput::try_from_solidity_paths(
        evm_version,
        input_files,
        libraries,
        remappings,
        SolcStandardJsonInputSettingsSelection::new_required(),
        SolcStandardJsonInputSettingsOptimizer::new(
            solc_optimizer_enabled,
            SolcStandardJsonInputSettingsOptimizer::default_mode(),
            Default::default(),
        ),
        Default::default(),
        suppressed_warnings,
        SolcStandardJsonInputSettingsPolkaVM::new(
            Some(memory_config),
            debug_config.emit_debug_info,
        ),
        llvm_arguments,
        false,
    )?;
    let mut solc_output = solc.standard_json(
        &mut solc_input,
        messages,
        base_path,
        include_paths,
        allow_paths,
    )?;
    solc_output.take_and_write_warnings();
    solc_output.check_errors()?;

    let linker_symbols = solc_input.settings.libraries.as_linker_symbols()?;

    let project = Project::try_from_standard_json_output(
        &mut solc_output,
        solc_input.settings.libraries,
        &solc_version,
        &debug_config,
    )?;
    solc_output.take_and_write_warnings();
    solc_output.check_errors()?;

    let mut build = project.compile(
        messages,
        optimizer_settings,
        metadata_hash,
        &debug_config,
        &solc_input.settings.llvm_arguments,
        memory_config,
    )?;
    build.take_and_write_warnings();
    build.check_errors()?;

    let mut build = build.link(linker_symbols, &debug_config);
    build.take_and_write_warnings();
    build.check_errors()?;

    Ok(build)
}

/// Runs the standard JSON mode.
pub fn standard_json<T: Compiler>(
    solc: &T,
    metadata_hash: MetadataHash,
    messages: &mut Vec<SolcStandardJsonOutputError>,
    json_path: Option<PathBuf>,
    base_path: Option<String>,
    include_paths: Vec<String>,
    allow_paths: Option<String>,
    debug_config: revive_llvm_context::DebugConfig,
    llvm_arguments: &[String],
    memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
    detect_missing_libraries: bool,
) -> anyhow::Result<()> {
    let solc_version = solc.version()?;
    let mut solc_input = SolcStandardJsonInput::try_from(json_path.as_deref())?;
    let language = solc_input.language;
    let prune_output = solc_input.settings.selection_to_prune();
    let deployed_libraries = solc_input.settings.libraries.as_paths();
    let linker_symbols = solc_input.settings.libraries.as_linker_symbols()?;
    let optimizer_settings = OptimizerSettings::try_from_cli(solc_input.settings.optimizer.mode)?;
    let detect_missing_libraries =
        solc_input.settings.detect_missing_libraries || detect_missing_libraries;

    solc_input.settings.llvm_arguments = llvm_arguments.to_owned();
    solc_input.extend_selection(SolcStandardJsonInputSettingsSelection::new_required());
    let mut solc_output = solc.standard_json(
        &mut solc_input,
        messages,
        base_path,
        include_paths,
        allow_paths,
    )?;

    let (mut solc_output, project) = match language {
        SolcStandardJsonInputLanguage::Solidity => {
            let project = Project::try_from_standard_json_output(
                &mut solc_output,
                solc_input.settings.libraries,
                &solc_version,
                &debug_config,
            )?;
            (solc_output, project)
        }
        SolcStandardJsonInputLanguage::Yul => {
            let mut solc_output = solc.validate_yul_standard_json(&mut solc_input, messages)?;
            if solc_output.has_errors() {
                solc_output.write_and_exit(prune_output);
            }
            let project = Project::try_from_yul_sources(
                solc_input.sources,
                solc_input.settings.libraries,
                Some(&mut solc_output),
                &debug_config,
            )?;

            (solc_output, project)
        }
    };

    if solc_output.has_errors() {
        solc_output.write_and_exit(prune_output);
    }

    if detect_missing_libraries {
        let missing_libraries = project.get_missing_libraries(&deployed_libraries);
        missing_libraries.write_to_standard_json(&mut solc_output, &solc_version);
        solc_output.write_and_exit(prune_output);
    }

    let build = project.compile(
        messages,
        optimizer_settings,
        metadata_hash,
        &debug_config,
        &solc_input.settings.llvm_arguments,
        memory_config,
    )?;
    if build.has_errors() {
        build.write_to_standard_json(&mut solc_output, &solc_version)?;
        solc_output.write_and_exit(prune_output);
    }

    let build = build.link(linker_symbols, &debug_config);
    build.write_to_standard_json(&mut solc_output, &solc_version)?;
    solc_output.write_and_exit(prune_output);
}

/// Runs the combined JSON mode.
pub fn combined_json<T: Compiler>(
    solc: &T,
    paths: &[PathBuf],
    libraries: &[String],
    metadata_hash: MetadataHash,
    messages: &mut Vec<SolcStandardJsonOutputError>,
    evm_version: Option<revive_common::EVMVersion>,
    format: String,
    solc_optimizer_enabled: bool,
    optimizer_settings: revive_llvm_context::OptimizerSettings,
    base_path: Option<String>,
    include_paths: Vec<String>,
    allow_paths: Option<String>,
    remappings: BTreeSet<String>,
    suppressed_warnings: Vec<ResolcWarning>,
    debug_config: revive_llvm_context::DebugConfig,
    output_directory: Option<PathBuf>,
    overwrite: bool,
    llvm_arguments: Vec<String>,
    memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
) -> anyhow::Result<()> {
    let selectors = CombinedJsonSelector::from_cli(format.as_str())
        .into_iter()
        .filter_map(|result| match result {
            Ok(selector) => Some(selector),
            Err(error) => {
                messages.push(SolcStandardJsonOutputError::new_error(
                    error.to_string(),
                    None,
                    None,
                ));
                None
            }
        })
        .collect::<HashSet<_>>();
    if !selectors.contains(&CombinedJsonSelector::Bytecode) {
        messages.push(SolcStandardJsonOutputError::new_warning(
            "Bytecode is always emitted even if the selector is not provided.".to_string(),
            None,
            None,
        ));
    }
    if selectors.contains(&CombinedJsonSelector::BytecodeRuntime) {
        messages.push(SolcStandardJsonOutputError::new_warning(
            format!("The `{}` selector does not make sense for the PVM target, since there is only one bytecode segment.", CombinedJsonSelector::BytecodeRuntime),
            None,
            None,
        ));
    }

    let mut combined_json = solc.combined_json(paths, selectors)?;
    standard_output(
        solc,
        paths,
        libraries,
        metadata_hash,
        messages,
        evm_version,
        solc_optimizer_enabled,
        optimizer_settings,
        base_path,
        include_paths,
        allow_paths,
        remappings,
        suppressed_warnings,
        debug_config,
        llvm_arguments,
        memory_config,
    )?
    .write_to_combined_json(&mut combined_json)?;

    match output_directory {
        Some(output_directory) => {
            std::fs::create_dir_all(output_directory.as_path())?;
            combined_json.write_to_directory(output_directory.as_path(), overwrite)?;

            writeln!(
                std::io::stderr(),
                "Compiler run successful. Artifact(s) can be found in directory {output_directory:?}."
            )?;
        }
        None => {
            serde_json::to_writer(std::io::stdout(), &combined_json)?;
        }
    }
    std::process::exit(revive_common::EXIT_CODE_SUCCESS);
}

/// Links unlinked bytecode files.
pub fn link(paths: Vec<String>, libraries: Vec<String>) -> anyhow::Result<()> {
    #[cfg(feature = "parallel")]
    let iter = paths.into_par_iter();
    #[cfg(not(feature = "parallel"))]
    let iter = paths.into_iter();

    let bytecodes = iter
        .map(|path| {
            let bytecode = std::fs::read(path.as_str())?;
            Ok((path, bytecode))
        })
        .collect::<anyhow::Result<BTreeMap<String, Vec<u8>>>>()?;

    let output = Linker::try_link(&bytecodes, &libraries)?;

    #[cfg(feature = "parallel")]
    let iter = output.linked.into_par_iter();
    #[cfg(not(feature = "parallel"))]
    let iter = output.linked.into_iter();

    iter.map(|(path, bytecode)| {
        std::fs::write(path, bytecode)?;
        Ok(())
    })
    .collect::<anyhow::Result<()>>()?;

    for (path, _) in output.unlinked {
        println!("Warning: file '{path}' still unresolved");
    }
    println!("Linking completed");

    std::process::exit(revive_common::EXIT_CODE_SUCCESS);
}
