//! Solidity to PolkaVM compiler library.

pub(crate) mod build;
pub(crate) mod r#const;
pub(crate) mod missing_libraries;
pub(crate) mod process;
pub(crate) mod project;
pub(crate) mod solc;
pub(crate) mod version;

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

#[cfg(not(target_os = "emscripten"))]
pub mod test_utils;
pub mod tests;

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;

use revive_common::MetadataHash;
use revive_llvm_context::OptimizerSettings;
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

/// Runs the Yul mode.
#[allow(clippy::too_many_arguments)]
pub fn yul(
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
    let linker_symbols = libraries.as_linker_symbols()?;
    let project = Project::try_from_yul_paths(input_files, None, libraries, &debug_config)?;
    let mut build = project.compile(
        messages,
        optimizer_settings,
        metadata_hash,
        debug_config,
        llvm_arguments,
        memory_config,
    )?;
    build.take_and_write_warnings();
    build.check_errors()?;

    let mut build = build.link(linker_symbols);
    build.take_and_write_warnings();
    build.check_errors()?;
    Ok(build)
}

/// Runs the LLVM IR mode.
pub fn llvm_ir(
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
    let linker_symbols = libraries.as_linker_symbols()?;

    let project = Project::try_from_llvm_ir_paths(input_files, libraries, None)?;

    let mut build = project.compile(
        messages,
        optimizer_settings,
        metadata_hash,
        debug_config,
        llvm_arguments,
        memory_config,
    )?;
    build.take_warnings();
    build.check_errors()?;

    let mut build = build.link(linker_symbols);
    build.take_and_write_warnings();
    build.check_errors()?;
    Ok(build)
}

/// Runs the standard output mode.
#[allow(clippy::too_many_arguments)]
pub fn standard_output<T: Compiler>(
    input_files: &[PathBuf],
    libraries: &[String],
    messages: &mut Vec<SolcStandardJsonOutputError>,
    metadata_hash: MetadataHash,
    solc: &mut T,
    evm_version: Option<revive_common::EVMVersion>,
    solc_optimizer_enabled: bool,
    optimizer_settings: revive_llvm_context::OptimizerSettings,
    base_path: Option<String>,
    include_paths: Vec<String>,
    allow_paths: Option<String>,
    remappings: BTreeSet<String>,
    suppressed_warnings: Vec<ResolcWarning>,
    debug_config: revive_llvm_context::DebugConfig,
    llvm_arguments: &[String],
    memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
) -> anyhow::Result<Build> {
    let solc_version = solc.version()?;
    let mut solc_input = SolcStandardJsonInput::try_from_solidity_paths(
        evm_version,
        input_files,
        libraries,
        remappings,
        SolcStandardJsonInputSettingsSelection::new_required(),
        SolcStandardJsonInputSettingsOptimizer::default(),
        Default::default(),
        suppressed_warnings,
        SolcStandardJsonInputSettingsPolkaVM::new(
            Some(memory_config),
            debug_config.emit_debug_info,
        ),
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
        debug_config,
        llvm_arguments,
        memory_config,
    )?;
    build.take_and_write_warnings();
    build.check_errors()?;

    let mut build = build.link(linker_symbols);
    build.take_and_write_warnings();
    build.check_errors()?;

    Ok(build)
}

/// Runs the standard JSON mode.
#[allow(clippy::too_many_arguments)]
pub fn standard_json<T: Compiler>(
    solc: &mut T,
    detect_missing_libraries: bool,
    messages: &mut Vec<SolcStandardJsonOutputError>,
    metadata_hash: MetadataHash,
    json_path: Option<PathBuf>,
    base_path: Option<String>,
    include_paths: Vec<String>,
    allow_paths: Option<String>,
    debug_config: revive_llvm_context::DebugConfig,
    llvm_arguments: &[String],
    memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
) -> anyhow::Result<()> {
    let solc_version = solc.version()?;
    let mut solc_input = SolcStandardJsonInput::try_from(json_path.as_deref())?;
    let language = solc_input.language;
    let prune_output = solc_input.settings.selection_to_prune();
    let deployed_libraries = solc_input.settings.libraries.as_paths();
    let linker_symbols = solc_input.settings.libraries.as_linker_symbols()?;

    let mut optimizer_settings =
        OptimizerSettings::try_from_cli(solc_input.settings.optimizer.mode)?;
    //let llvm_options = solc_input.settings.llvm_options.clone();

    let detect_missing_libraries =
        solc_input.settings.detect_missing_libraries || detect_missing_libraries;

    solc_input.extend_selection(SolcStandardJsonInputSettingsSelection::new_required());
    let mut solc_output = solc.standard_json(
        &mut solc_input,
        messages,
        base_path,
        include_paths,
        allow_paths,
    )?;

    let project = Project::try_from_standard_json_output(
        &mut solc_output,
        solc_input.settings.libraries,
        &solc_version,
        &debug_config,
    )?;
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
        debug_config,
        llvm_arguments,
        memory_config,
    )?;
    if build.has_errors() {
        build.write_to_standard_json(&mut solc_output, &solc_version)?;
        solc_output.write_and_exit(prune_output);
    }

    let build = build.link(linker_symbols);
    build.write_to_standard_json(&mut solc_output, &solc_version)?;
    solc_output.write_and_exit(prune_output);
}

/// Runs the combined JSON mode.
#[allow(clippy::too_many_arguments)]
pub fn combined_json<T: Compiler>(
    format: String,
    input_files: &[PathBuf],
    libraries: &[String],
    messages: &mut Vec<SolcStandardJsonOutputError>,
    metadata_hash: MetadataHash,
    solc: &mut T,
    evm_version: Option<revive_common::EVMVersion>,
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
    llvm_arguments: &[String],
    memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
) -> anyhow::Result<()> {
    let build = standard_output(
        input_files,
        libraries,
        messages,
        metadata_hash,
        solc,
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
    )?;

    let mut combined_json = solc.combined_json(input_files, format.as_str())?;
    build.write_to_combined_json(&mut combined_json)?;

    match output_directory {
        Some(output_directory) => {
            std::fs::create_dir_all(output_directory.as_path())?;

            combined_json.write_to_directory(output_directory.as_path(), overwrite)?;
        }
        None => {
            writeln!(
                std::io::stdout(),
                "{}",
                serde_json::to_string(&combined_json).expect("Always valid")
            )?;
        }
    }
    std::process::exit(0);
}
