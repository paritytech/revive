//! Solidity to PolkaVM compiler library.

pub(crate) mod build;
pub(crate) mod r#const;
pub(crate) mod evmla;
pub(crate) mod missing_libraries;
pub(crate) mod process;
pub(crate) mod project;
pub(crate) mod solc;
pub(crate) mod version;
pub(crate) mod warning;
pub(crate) mod yul;

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
pub use self::solc::combined_json::contract::Contract as SolcCombinedJsonContract;
pub use self::solc::combined_json::CombinedJson as SolcCombinedJson;
pub use self::solc::pipeline::Pipeline as SolcPipeline;
#[cfg(not(target_os = "emscripten"))]
pub use self::solc::solc_compiler::SolcCompiler;
#[cfg(target_os = "emscripten")]
pub use self::solc::soljson_compiler::SoljsonCompiler;
pub use self::solc::standard_json::input::language::Language as SolcStandardJsonInputLanguage;
pub use self::solc::standard_json::input::settings::metadata::Metadata as SolcStandardJsonInputSettingsMetadata;
pub use self::solc::standard_json::input::settings::optimizer::Optimizer as SolcStandardJsonInputSettingsOptimizer;
pub use self::solc::standard_json::input::settings::selection::file::flag::Flag as SolcStandardJsonInputSettingsSelectionFileFlag;
pub use self::solc::standard_json::input::settings::selection::file::File as SolcStandardJsonInputSettingsSelectionFile;
pub use self::solc::standard_json::input::settings::selection::Selection as SolcStandardJsonInputSettingsSelection;
pub use self::solc::standard_json::input::settings::Settings as SolcStandardJsonInputSettings;
pub use self::solc::standard_json::input::source::Source as SolcStandardJsonInputSource;
pub use self::solc::standard_json::input::Input as SolcStandardJsonInput;
pub use self::solc::standard_json::output::contract::evm::bytecode::Bytecode as SolcStandardJsonOutputContractEVMBytecode;
pub use self::solc::standard_json::output::contract::evm::EVM as SolcStandardJsonOutputContractEVM;
pub use self::solc::standard_json::output::contract::Contract as SolcStandardJsonOutputContract;
pub use self::solc::standard_json::output::Output as SolcStandardJsonOutput;
pub use self::solc::version::Version as SolcVersion;
pub use self::solc::Compiler;
pub use self::version::Version as ResolcVersion;
pub use self::warning::Warning;
#[cfg(not(target_os = "emscripten"))]
pub mod test_utils;
pub mod tests;

use std::collections::BTreeSet;
use std::path::PathBuf;

/// Runs the Yul mode.
pub fn yul<T: Compiler>(
    input_files: &[PathBuf],
    solc: &mut T,
    optimizer_settings: revive_llvm_context::OptimizerSettings,
    include_metadata_hash: bool,
    debug_config: revive_llvm_context::DebugConfig,
) -> anyhow::Result<Build> {
    let path = match input_files.len() {
        1 => input_files.first().expect("Always exists"),
        0 => anyhow::bail!("The input file is missing"),
        length => anyhow::bail!(
            "Only one input file is allowed in the Yul mode, but found {}",
            length,
        ),
    };

    if solc.version()?.default != solc::LAST_SUPPORTED_VERSION {
        anyhow::bail!(
                "The Yul mode is only supported with the most recent version of the Solidity compiler: {}",
                solc::LAST_SUPPORTED_VERSION,
            );
    }

    let solc_validator = Some(&*solc);
    let project = Project::try_from_yul_path(path, solc_validator)?;

    let build = project.compile(optimizer_settings, include_metadata_hash, debug_config)?;

    Ok(build)
}

/// Runs the LLVM IR mode.
pub fn llvm_ir(
    input_files: &[PathBuf],
    optimizer_settings: revive_llvm_context::OptimizerSettings,
    include_metadata_hash: bool,
    debug_config: revive_llvm_context::DebugConfig,
) -> anyhow::Result<Build> {
    let path = match input_files.len() {
        1 => input_files.first().expect("Always exists"),
        0 => anyhow::bail!("The input file is missing"),
        length => anyhow::bail!(
            "Only one input file is allowed in the LLVM IR mode, but found {}",
            length,
        ),
    };

    let project = Project::try_from_llvm_ir_path(path)?;

    let build = project.compile(optimizer_settings, include_metadata_hash, debug_config)?;

    Ok(build)
}

/// Runs the standard output mode.
#[allow(clippy::too_many_arguments)]
pub fn standard_output<T: Compiler>(
    input_files: &[PathBuf],
    libraries: Vec<String>,
    solc: &mut T,
    evm_version: Option<revive_common::EVMVersion>,
    solc_optimizer_enabled: bool,
    optimizer_settings: revive_llvm_context::OptimizerSettings,
    force_evmla: bool,
    include_metadata_hash: bool,
    base_path: Option<String>,
    include_paths: Vec<String>,
    allow_paths: Option<String>,
    remappings: Option<BTreeSet<String>>,
    suppressed_warnings: Option<Vec<Warning>>,
    debug_config: revive_llvm_context::DebugConfig,
) -> anyhow::Result<Build> {
    let solc_version = solc.version()?;
    let solc_pipeline = SolcPipeline::new(&solc_version, force_evmla);

    let solc_input = SolcStandardJsonInput::try_from_paths(
        SolcStandardJsonInputLanguage::Solidity,
        evm_version,
        input_files,
        libraries,
        remappings,
        SolcStandardJsonInputSettingsSelection::new_required(solc_pipeline),
        SolcStandardJsonInputSettingsOptimizer::new(
            solc_optimizer_enabled,
            None,
            &solc_version.default,
            optimizer_settings.is_fallback_to_size_enabled(),
        ),
        None,
        solc_pipeline == SolcPipeline::Yul,
        suppressed_warnings,
    )?;

    let source_code_files = solc_input
        .sources
        .iter()
        .map(|(path, source)| (path.to_owned(), source.content.to_owned()))
        .collect();

    let libraries = solc_input.settings.libraries.clone().unwrap_or_default();
    let mut solc_output = solc.standard_json(
        solc_input,
        solc_pipeline,
        base_path,
        include_paths,
        allow_paths,
    )?;

    if let Some(errors) = solc_output.errors.as_deref() {
        let mut has_errors = false;

        for error in errors.iter() {
            if error.severity.as_str() == "error" {
                has_errors = true;
            }

            eprintln!("{error}");
        }

        if has_errors {
            anyhow::bail!("Error(s) found. Compilation aborted");
        }
    }

    let project = solc_output.try_to_project(
        source_code_files,
        libraries,
        solc_pipeline,
        &solc_version,
        &debug_config,
    )?;

    let build = project.compile(optimizer_settings, include_metadata_hash, debug_config)?;

    Ok(build)
}

/// Runs the standard JSON mode.
#[allow(clippy::too_many_arguments)]
pub fn standard_json<T: Compiler>(
    solc: &mut T,
    detect_missing_libraries: bool,
    force_evmla: bool,
    base_path: Option<String>,
    include_paths: Vec<String>,
    allow_paths: Option<String>,
    debug_config: revive_llvm_context::DebugConfig,
) -> anyhow::Result<()> {
    let solc_version = solc.version()?;
    let solc_pipeline = SolcPipeline::new(&solc_version, force_evmla);

    let solc_input = SolcStandardJsonInput::try_from_stdin(solc_pipeline)?;
    let source_code_files = solc_input
        .sources
        .iter()
        .map(|(path, source)| (path.to_owned(), source.content.to_owned()))
        .collect();

    let optimizer_settings =
        revive_llvm_context::OptimizerSettings::try_from(&solc_input.settings.optimizer)?;

    let include_metadata_hash = match solc_input.settings.metadata {
        Some(ref metadata) => {
            metadata.bytecode_hash != Some(revive_llvm_context::PolkaVMMetadataHash::None)
        }
        None => true,
    };

    let libraries = solc_input.settings.libraries.clone().unwrap_or_default();
    let mut solc_output = solc.standard_json(
        solc_input,
        solc_pipeline,
        base_path,
        include_paths,
        allow_paths,
    )?;

    if let Some(errors) = solc_output.errors.as_deref() {
        for error in errors.iter() {
            if error.severity.as_str() == "error" {
                serde_json::to_writer(std::io::stdout(), &solc_output)?;
                std::process::exit(0);
            }
        }
    }

    let project = solc_output.try_to_project(
        source_code_files,
        libraries,
        solc_pipeline,
        &solc_version,
        &debug_config,
    )?;

    if detect_missing_libraries {
        let missing_libraries = project.get_missing_libraries();
        missing_libraries.write_to_standard_json(&mut solc_output, &solc_version)?;
    } else {
        let build = project.compile(optimizer_settings, include_metadata_hash, debug_config)?;
        build.write_to_standard_json(&mut solc_output, &solc_version)?;
    }
    serde_json::to_writer(std::io::stdout(), &solc_output)?;
    std::process::exit(0);
}

/// Runs the combined JSON mode.
#[allow(clippy::too_many_arguments)]
pub fn combined_json<T: Compiler>(
    format: String,
    input_files: &[PathBuf],
    libraries: Vec<String>,
    solc: &mut T,
    evm_version: Option<revive_common::EVMVersion>,
    solc_optimizer_enabled: bool,
    optimizer_settings: revive_llvm_context::OptimizerSettings,
    force_evmla: bool,
    include_metadata_hash: bool,
    base_path: Option<String>,
    include_paths: Vec<String>,
    allow_paths: Option<String>,
    remappings: Option<BTreeSet<String>>,
    suppressed_warnings: Option<Vec<Warning>>,
    debug_config: revive_llvm_context::DebugConfig,
    output_directory: Option<PathBuf>,
    overwrite: bool,
) -> anyhow::Result<()> {
    let build = standard_output(
        input_files,
        libraries,
        solc,
        evm_version,
        solc_optimizer_enabled,
        optimizer_settings,
        force_evmla,
        include_metadata_hash,
        base_path,
        include_paths,
        allow_paths,
        remappings,
        suppressed_warnings,
        debug_config,
    )?;

    let mut combined_json = solc.combined_json(input_files, format.as_str())?;
    build.write_to_combined_json(&mut combined_json)?;

    match output_directory {
        Some(output_directory) => {
            std::fs::create_dir_all(output_directory.as_path())?;

            combined_json.write_to_directory(output_directory.as_path(), overwrite)?;
        }
        None => {
            println!(
                "{}",
                serde_json::to_string(&combined_json).expect("Always valid")
            );
        }
    }
    std::process::exit(0);
}
