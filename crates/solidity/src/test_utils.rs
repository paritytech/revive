//! Common utility used for in frontend and integration tests.
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::str::FromStr;

use crate::compiler::pipeline::Pipeline as SolcPipeline;
use crate::compiler::solc::SolcCompiler;
use crate::compiler::standard_json::input::settings::optimizer::Optimizer as SolcStandardJsonInputSettingsOptimizer;
use crate::compiler::standard_json::input::settings::selection::Selection as SolcStandardJsonInputSettingsSelection;
use crate::compiler::standard_json::input::Input as SolcStandardJsonInput;
use crate::compiler::standard_json::output::contract::evm::bytecode::DeployedBytecode;
use crate::compiler::standard_json::output::Output as SolcStandardJsonOutput;
use crate::compiler::Compiler;
use crate::project::Project;
use crate::warning::Warning;

/// Checks if the required executables are present in `${PATH}`.
fn check_dependencies() {
    for executable in [
        crate::r#const::DEFAULT_EXECUTABLE_NAME,
        SolcCompiler::DEFAULT_EXECUTABLE_NAME,
    ]
    .iter()
    {
        assert!(
            which::which(executable).is_ok(),
            "The `{executable}` executable not found in ${{PATH}}"
        );
    }
}

/// Builds the Solidity project and returns the standard JSON output.
pub fn build_solidity(
    sources: BTreeMap<String, String>,
    libraries: BTreeMap<String, BTreeMap<String, String>>,
    remappings: Option<BTreeSet<String>>,
    pipeline: SolcPipeline,
    optimizer_settings: revive_llvm_context::OptimizerSettings,
) -> anyhow::Result<SolcStandardJsonOutput> {
    build_solidity_with_options(
        sources,
        libraries,
        remappings,
        pipeline,
        optimizer_settings,
        true,
    )
}

/// Builds the Solidity project and returns the standard JSON output.
/// Gives control over additional options:
/// - `solc_optimizer_enabled`: Whether to use the `solc` optimizer
pub fn build_solidity_with_options(
    sources: BTreeMap<String, String>,
    libraries: BTreeMap<String, BTreeMap<String, String>>,
    remappings: Option<BTreeSet<String>>,
    pipeline: SolcPipeline,
    optimizer_settings: revive_llvm_context::OptimizerSettings,
    solc_optimizer_enabled: bool,
) -> anyhow::Result<SolcStandardJsonOutput> {
    check_dependencies();

    inkwell::support::enable_llvm_pretty_stack_trace();
    revive_llvm_context::initialize_target(revive_llvm_context::Target::PVM);
    let _ = crate::process::native_process::EXECUTABLE
        .set(PathBuf::from(crate::r#const::DEFAULT_EXECUTABLE_NAME));

    let mut solc = SolcCompiler::new(SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned())?;
    let solc_version = solc.version()?;

    let input = SolcStandardJsonInput::try_from_sources(
        None,
        sources.clone(),
        libraries.clone(),
        remappings,
        SolcStandardJsonInputSettingsSelection::new_required(pipeline),
        SolcStandardJsonInputSettingsOptimizer::new(
            solc_optimizer_enabled,
            None,
            &solc_version.default,
            false,
            false,
        ),
        None,
        pipeline == SolcPipeline::Yul,
        None,
    )?;

    let mut output = solc.standard_json(input, pipeline, None, vec![], None)?;

    let project = output.try_to_project(sources, libraries, pipeline, &solc_version, None)?;

    let build: crate::Build = project.compile(optimizer_settings, false, false, false, None)?;
    build.write_to_standard_json(
        &mut output,
        &solc_version,
        &semver::Version::from_str(env!("CARGO_PKG_VERSION"))?,
    )?;

    Ok(output)
}

/// Build a Solidity contract and get the EVM bin-runtime.
pub fn build_solidity_with_options_evm(
    sources: BTreeMap<String, String>,
    libraries: BTreeMap<String, BTreeMap<String, String>>,
    remappings: Option<BTreeSet<String>>,
    pipeline: SolcPipeline,
    solc_optimizer_enabled: bool,
) -> anyhow::Result<BTreeMap<String, DeployedBytecode>> {
    check_dependencies();

    inkwell::support::enable_llvm_pretty_stack_trace();
    revive_llvm_context::initialize_target(revive_llvm_context::Target::PVM);
    let _ = crate::process::native_process::EXECUTABLE
        .set(PathBuf::from(crate::r#const::DEFAULT_EXECUTABLE_NAME));

    let mut solc = SolcCompiler::new(SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned())?;
    let solc_version = solc.version()?;

    let input = SolcStandardJsonInput::try_from_sources(
        None,
        sources.clone(),
        libraries.clone(),
        remappings,
        SolcStandardJsonInputSettingsSelection::new_required(pipeline),
        SolcStandardJsonInputSettingsOptimizer::new(
            solc_optimizer_enabled,
            None,
            &solc_version.default,
            false,
            false,
        ),
        None,
        pipeline == SolcPipeline::Yul,
        None,
    )?;

    let mut output = solc.standard_json(input, pipeline, None, vec![], None)?;

    let mut contracts = BTreeMap::new();
    if let Some(files) = output.contracts.as_mut() {
        for (_, file) in files.iter_mut() {
            for (name, contract) in file.iter_mut() {
                if let Some(evm) = contract.evm.as_mut() {
                    if let Some(deployed_bytecode) = evm.deployed_bytecode.as_ref() {
                        contracts.insert(name.clone(), deployed_bytecode.clone());
                    }
                }
            }
        }
    }

    Ok(contracts)
}

/// Builds the Solidity project and returns the standard JSON output.
pub fn build_solidity_and_detect_missing_libraries(
    sources: BTreeMap<String, String>,
    libraries: BTreeMap<String, BTreeMap<String, String>>,
    pipeline: SolcPipeline,
) -> anyhow::Result<SolcStandardJsonOutput> {
    check_dependencies();

    inkwell::support::enable_llvm_pretty_stack_trace();
    revive_llvm_context::initialize_target(revive_llvm_context::Target::PVM);
    let _ = crate::process::native_process::EXECUTABLE
        .set(PathBuf::from(crate::r#const::DEFAULT_EXECUTABLE_NAME));

    let mut solc = SolcCompiler::new(SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned())?;
    let solc_version = solc.version()?;

    let input = SolcStandardJsonInput::try_from_sources(
        None,
        sources.clone(),
        libraries.clone(),
        None,
        SolcStandardJsonInputSettingsSelection::new_required(pipeline),
        SolcStandardJsonInputSettingsOptimizer::new(
            true,
            None,
            &solc_version.default,
            false,
            false,
        ),
        None,
        pipeline == SolcPipeline::Yul,
        None,
    )?;

    let mut output = solc.standard_json(input, pipeline, None, vec![], None)?;

    let project = output.try_to_project(sources, libraries, pipeline, &solc_version, None)?;

    let missing_libraries = project.get_missing_libraries();
    missing_libraries.write_to_standard_json(
        &mut output,
        &solc.version()?,
        &semver::Version::from_str(env!("CARGO_PKG_VERSION"))?,
    )?;

    Ok(output)
}

/// Checks if the Yul project can be built without errors.
pub fn build_yul(source_code: &str) -> anyhow::Result<()> {
    check_dependencies();

    inkwell::support::enable_llvm_pretty_stack_trace();
    revive_llvm_context::initialize_target(revive_llvm_context::Target::PVM);
    let optimizer_settings = revive_llvm_context::OptimizerSettings::none();

    let project = Project::try_from_yul_string::<SolcCompiler>(
        PathBuf::from("test.yul").as_path(),
        source_code,
        None,
    )?;
    let _build = project.compile(optimizer_settings, false, false, false, None)?;

    Ok(())
}

/// Checks if the built Solidity project contains the given warning.
pub fn check_solidity_warning(
    source_code: &str,
    warning_substring: &str,
    libraries: BTreeMap<String, BTreeMap<String, String>>,
    pipeline: SolcPipeline,
    skip_for_zkvm_edition: bool,
    suppressed_warnings: Option<Vec<Warning>>,
) -> anyhow::Result<bool> {
    check_dependencies();

    let mut solc = SolcCompiler::new(SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned())?;
    let solc_version = solc.version()?;
    if skip_for_zkvm_edition && solc_version.l2_revision.is_some() {
        return Ok(true);
    }

    let mut sources = BTreeMap::new();
    sources.insert("test.sol".to_string(), source_code.to_string());
    let input = SolcStandardJsonInput::try_from_sources(
        None,
        sources.clone(),
        libraries,
        None,
        SolcStandardJsonInputSettingsSelection::new_required(pipeline),
        SolcStandardJsonInputSettingsOptimizer::new(
            true,
            None,
            &solc_version.default,
            false,
            false,
        ),
        None,
        pipeline == SolcPipeline::Yul,
        suppressed_warnings,
    )?;

    let output = solc.standard_json(input, pipeline, None, vec![], None)?;
    let contains_warning = output
        .errors
        .ok_or_else(|| anyhow::anyhow!("Solidity compiler messages not found"))?
        .iter()
        .any(|error| error.formatted_message.contains(warning_substring));

    Ok(contains_warning)
}
