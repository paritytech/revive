//! Common utility used for in frontend and integration tests.
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use revive_llvm_context::OptimizerSettings;
use revive_solc_json_interface::standard_json::output::contract::evm::bytecode::Bytecode;
use revive_solc_json_interface::standard_json::output::contract::evm::bytecode::DeployedBytecode;
use revive_solc_json_interface::warning::Warning;
use revive_solc_json_interface::SolcStandardJsonInput;
use revive_solc_json_interface::SolcStandardJsonInputSettingsOptimizer;
use revive_solc_json_interface::SolcStandardJsonInputSettingsSelection;
use revive_solc_json_interface::SolcStandardJsonOutput;

use crate::project::Project;
use crate::solc::solc_compiler::SolcCompiler;
use crate::solc::Compiler;

static PVM_BLOB_CACHE: Lazy<Mutex<HashMap<CachedBlob, Vec<u8>>>> = Lazy::new(Default::default);
static EVM_BLOB_CACHE: Lazy<Mutex<HashMap<CachedBlob, Vec<u8>>>> = Lazy::new(Default::default);
static EVM_RUNTIME_BLOB_CACHE: Lazy<Mutex<HashMap<CachedBlob, Vec<u8>>>> =
    Lazy::new(Default::default);

const DEBUG_CONFIG: revive_llvm_context::DebugConfig =
    revive_llvm_context::DebugConfig::new(None, true);

#[derive(Hash, PartialEq, Eq)]
struct CachedBlob {
    contract_name: String,
    solidity: String,
    solc_optimizer_enabled: bool,
    opt: String,
}

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
    optimizer_settings: revive_llvm_context::OptimizerSettings,
) -> anyhow::Result<SolcStandardJsonOutput> {
    build_solidity_with_options(sources, libraries, remappings, optimizer_settings, true)
}

/// Builds the Solidity project and returns the standard JSON output.
/// Gives control over additional options:
/// - `solc_optimizer_enabled`: Whether to use the `solc` optimizer
pub fn build_solidity_with_options(
    sources: BTreeMap<String, String>,
    libraries: BTreeMap<String, BTreeMap<String, String>>,
    remappings: Option<BTreeSet<String>>,
    optimizer_settings: revive_llvm_context::OptimizerSettings,
    solc_optimizer_enabled: bool,
) -> anyhow::Result<SolcStandardJsonOutput> {
    check_dependencies();

    inkwell::support::enable_llvm_pretty_stack_trace();
    revive_llvm_context::initialize_llvm(
        revive_llvm_context::Target::PVM,
        crate::DEFAULT_EXECUTABLE_NAME,
        &[],
    );
    let _ = crate::process::native_process::EXECUTABLE
        .set(PathBuf::from(crate::r#const::DEFAULT_EXECUTABLE_NAME));

    let mut solc = SolcCompiler::new(SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned())?;
    let solc_version = solc.version()?;

    let input = SolcStandardJsonInput::try_from_sources(
        None,
        sources.clone(),
        libraries.clone(),
        remappings,
        SolcStandardJsonInputSettingsSelection::new_required(),
        SolcStandardJsonInputSettingsOptimizer::new(
            solc_optimizer_enabled,
            optimizer_settings.middle_end_as_string().chars().last(),
            &solc_version.default,
            false,
        ),
        None,
        None,
    )?;

    let mut output = solc.standard_json(input, None, vec![], None)?;

    let debug_config = revive_llvm_context::DebugConfig::new(
        None,
        optimizer_settings.middle_end_as_string() != "z",
    );

    let project = Project::try_from_standard_json_output(
        &output,
        sources,
        libraries,
        &solc_version,
        &debug_config,
    )?;

    let build: crate::Build = project.compile(
        optimizer_settings,
        false,
        debug_config,
        Default::default(),
        Default::default(),
    )?;
    build.write_to_standard_json(&mut output, &solc_version)?;

    Ok(output)
}

/// Build a Solidity contract and get the EVM code
pub fn build_solidity_with_options_evm(
    sources: BTreeMap<String, String>,
    libraries: BTreeMap<String, BTreeMap<String, String>>,
    remappings: Option<BTreeSet<String>>,
    solc_optimizer_enabled: bool,
) -> anyhow::Result<BTreeMap<String, (Bytecode, DeployedBytecode)>> {
    check_dependencies();

    inkwell::support::enable_llvm_pretty_stack_trace();
    revive_llvm_context::initialize_llvm(
        revive_llvm_context::Target::PVM,
        crate::DEFAULT_EXECUTABLE_NAME,
        &[],
    );
    let _ = crate::process::native_process::EXECUTABLE
        .set(PathBuf::from(crate::r#const::DEFAULT_EXECUTABLE_NAME));

    let mut solc = SolcCompiler::new(SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned())?;
    let solc_version = solc.version()?;

    let input = SolcStandardJsonInput::try_from_sources(
        None,
        sources.clone(),
        libraries.clone(),
        remappings,
        SolcStandardJsonInputSettingsSelection::new_required(),
        SolcStandardJsonInputSettingsOptimizer::new(
            solc_optimizer_enabled,
            None,
            &solc_version.default,
            false,
        ),
        None,
        None,
    )?;

    let mut output = solc.standard_json(input, None, vec![], None)?;

    let mut contracts = BTreeMap::new();
    if let Some(files) = output.contracts.as_mut() {
        for (_, file) in files.iter_mut() {
            for (name, contract) in file.iter_mut() {
                if let Some(evm) = contract.evm.as_mut() {
                    let (Some(bytecode), Some(deployed_bytecode)) =
                        (evm.bytecode.as_ref(), evm.deployed_bytecode.as_ref())
                    else {
                        continue;
                    };
                    contracts.insert(name.clone(), (bytecode.clone(), deployed_bytecode.clone()));
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
) -> anyhow::Result<SolcStandardJsonOutput> {
    check_dependencies();

    inkwell::support::enable_llvm_pretty_stack_trace();
    revive_llvm_context::initialize_llvm(
        revive_llvm_context::Target::PVM,
        crate::DEFAULT_EXECUTABLE_NAME,
        &[],
    );
    let _ = crate::process::native_process::EXECUTABLE
        .set(PathBuf::from(crate::r#const::DEFAULT_EXECUTABLE_NAME));

    let mut solc = SolcCompiler::new(SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned())?;
    let solc_version = solc.version()?;

    let input = SolcStandardJsonInput::try_from_sources(
        None,
        sources.clone(),
        libraries.clone(),
        None,
        SolcStandardJsonInputSettingsSelection::new_required(),
        SolcStandardJsonInputSettingsOptimizer::new(true, None, &solc_version.default, false),
        None,
        None,
    )?;

    let mut output = solc.standard_json(input, None, vec![], None)?;

    let project = Project::try_from_standard_json_output(
        &output,
        sources,
        libraries,
        &solc_version,
        &DEBUG_CONFIG,
    )?;

    let missing_libraries = project.get_missing_libraries();
    missing_libraries.write_to_standard_json(&mut output, &solc.version()?)?;

    Ok(output)
}

/// Checks if the Yul project can be built without errors.
pub fn build_yul(source_code: &str) -> anyhow::Result<()> {
    check_dependencies();

    inkwell::support::enable_llvm_pretty_stack_trace();
    revive_llvm_context::initialize_llvm(
        revive_llvm_context::Target::PVM,
        crate::DEFAULT_EXECUTABLE_NAME,
        &[],
    );
    let optimizer_settings = revive_llvm_context::OptimizerSettings::none();

    let project = Project::try_from_yul_string::<SolcCompiler>(
        PathBuf::from("test.yul").as_path(),
        source_code,
        None,
    )?;
    let _build = project.compile(
        optimizer_settings,
        false,
        DEBUG_CONFIG,
        Default::default(),
        Default::default(),
    )?;

    Ok(())
}

/// Checks if the built Solidity project contains the given warning.
pub fn check_solidity_warning(
    source_code: &str,
    warning_substring: &str,
    libraries: BTreeMap<String, BTreeMap<String, String>>,
    skip_for_revive_edition: bool,
    suppressed_warnings: Option<Vec<Warning>>,
) -> anyhow::Result<bool> {
    check_dependencies();

    let mut solc = SolcCompiler::new(SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned())?;
    let solc_version = solc.version()?;
    if skip_for_revive_edition && solc_version.l2_revision.is_some() {
        return Ok(true);
    }

    let mut sources = BTreeMap::new();
    sources.insert("test.sol".to_string(), source_code.to_string());
    let input = SolcStandardJsonInput::try_from_sources(
        None,
        sources.clone(),
        libraries,
        None,
        SolcStandardJsonInputSettingsSelection::new_required(),
        SolcStandardJsonInputSettingsOptimizer::new(true, None, &solc_version.default, false),
        None,
        suppressed_warnings,
    )?;

    let output = solc.standard_json(input, None, vec![], None)?;
    let contains_warning = output
        .errors
        .ok_or_else(|| anyhow::anyhow!("Solidity compiler messages not found"))?
        .iter()
        .any(|error| error.formatted_message.contains(warning_substring));

    Ok(contains_warning)
}

/// Compile the blob of `contract_name` found in given `source_code`.
/// The `solc` optimizer will be enabled
pub fn compile_blob(contract_name: &str, source_code: &str) -> Vec<u8> {
    compile_blob_with_options(
        contract_name,
        source_code,
        true,
        OptimizerSettings::cycles(),
    )
}

/// Compile the EVM bin-runtime of `contract_name` found in given `source_code`.
/// The `solc` optimizer will be enabled
pub fn compile_evm_bin_runtime(contract_name: &str, source_code: &str) -> Vec<u8> {
    compile_evm(contract_name, source_code, true, true)
}

/// Compile the EVM bin of `contract_name` found in given `source_code`.
/// The `solc` optimizer will be enabled
pub fn compile_evm_deploy_code(
    contract_name: &str,
    source_code: &str,
    solc_optimizer_enabled: bool,
) -> Vec<u8> {
    compile_evm(contract_name, source_code, solc_optimizer_enabled, false)
}

fn compile_evm(
    contract_name: &str,
    source_code: &str,
    solc_optimizer_enabled: bool,
    runtime: bool,
) -> Vec<u8> {
    let id = CachedBlob {
        contract_name: contract_name.to_owned(),
        solidity: source_code.to_owned(),
        solc_optimizer_enabled,
        opt: String::new(),
    };

    let cache = if runtime {
        &EVM_RUNTIME_BLOB_CACHE
    } else {
        &EVM_BLOB_CACHE
    };
    if let Some(blob) = cache.lock().unwrap().get(&id) {
        return blob.clone();
    }

    let file_name = "contract.sol";
    let contracts = build_solidity_with_options_evm(
        [(file_name.into(), source_code.into())].into(),
        Default::default(),
        None,
        solc_optimizer_enabled,
    )
    .expect("source should compile");
    let object = &contracts
        .get(contract_name)
        .unwrap_or_else(|| panic!("contract '{}' didn't produce bin-runtime", contract_name));
    let code = if runtime {
        object.1.object.as_str()
    } else {
        object.0.object.as_str()
    };
    let blob = hex::decode(code).expect("code shold be hex encoded");

    cache.lock().unwrap().insert(id, blob.clone());

    blob
}

/// Compile the blob of `contract_name` found in given `source_code`.
pub fn compile_blob_with_options(
    contract_name: &str,
    source_code: &str,
    solc_optimizer_enabled: bool,
    optimizer_settings: revive_llvm_context::OptimizerSettings,
) -> Vec<u8> {
    let id = CachedBlob {
        contract_name: contract_name.to_owned(),
        solidity: source_code.to_owned(),
        solc_optimizer_enabled,
        opt: optimizer_settings.middle_end_as_string(),
    };

    if let Some(blob) = PVM_BLOB_CACHE.lock().unwrap().get(&id) {
        return blob.clone();
    }

    let file_name = "contract.sol";
    let contracts = build_solidity_with_options(
        [(file_name.into(), source_code.into())].into(),
        Default::default(),
        None,
        optimizer_settings,
        solc_optimizer_enabled,
    )
    .expect("source should compile")
    .contracts
    .expect("source should contain at least one contract");

    let bytecode = contracts[file_name][contract_name]
        .evm
        .as_ref()
        .expect("source should produce EVM output")
        .bytecode
        .as_ref()
        .expect("source should produce assembly text")
        .object
        .as_str();
    let blob = hex::decode(bytecode).expect("hex encoding should always be valid");

    PVM_BLOB_CACHE.lock().unwrap().insert(id, blob.clone());

    blob
}
