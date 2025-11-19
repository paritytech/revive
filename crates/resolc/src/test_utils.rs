//! Common helper utilities used in tests and benchmarks.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use revive_common::MetadataHash;
use revive_llvm_context::initialize_llvm;
use revive_llvm_context::DebugConfig;
use revive_llvm_context::OptimizerSettings;
use revive_llvm_context::PolkaVMTarget;
use revive_solc_json_interface::standard_json::output::contract::evm::bytecode::Bytecode;
use revive_solc_json_interface::standard_json::output::contract::evm::bytecode::DeployedBytecode;
use revive_solc_json_interface::ResolcWarning;
use revive_solc_json_interface::SolcStandardJsonInput;
use revive_solc_json_interface::SolcStandardJsonInputSettingsLibraries;
use revive_solc_json_interface::SolcStandardJsonInputSettingsMetadata;
use revive_solc_json_interface::SolcStandardJsonInputSettingsOptimizer;
use revive_solc_json_interface::SolcStandardJsonInputSettingsSelection;
use revive_solc_json_interface::SolcStandardJsonInputSource;
use revive_solc_json_interface::SolcStandardJsonOutput;
use revive_solc_json_interface::SolcStandardJsonOutputContract;
use revive_solc_json_interface::SolcStandardJsonOutputErrorHandler;

use crate::project::Project;
use crate::solc::solc_compiler::SolcCompiler;
use crate::solc::Compiler;

static PVM_BLOB_CACHE: Lazy<Mutex<HashMap<CachedBlob, Vec<u8>>>> = Lazy::new(Default::default);
static EVM_BLOB_CACHE: Lazy<Mutex<HashMap<CachedBlob, Vec<u8>>>> = Lazy::new(Default::default);
static EVM_RUNTIME_BLOB_CACHE: Lazy<Mutex<HashMap<CachedBlob, Vec<u8>>>> =
    Lazy::new(Default::default);
static YUL_IR_CACHE: Lazy<Mutex<HashMap<CachedBlob, String>>> = Lazy::new(Default::default);

const DEBUG_CONFIG: revive_llvm_context::DebugConfig = DebugConfig::new(None, true);

/// Tests may share and re-use contract code.
/// The compiled blob cache helps avoiding duplicate compilation.
#[derive(Hash, PartialEq, Eq)]
struct CachedBlob {
    /// The contract name.
    contract_name: String,
    /// Whether the solc optimizer is enabled.
    solc_optimizer_enabled: bool,
    /// The contract code.
    solidity: String,
    /// The optimization level.
    opt: String,
}

/// Builds the Solidity project and returns the standard JSON output.
pub fn build_solidity(
    sources: BTreeMap<String, SolcStandardJsonInputSource>,
) -> anyhow::Result<SolcStandardJsonOutput> {
    build_solidity_with_options(
        sources,
        Default::default(),
        Default::default(),
        OptimizerSettings::cycles(),
        true,
        Default::default(),
    )
}

/// Builds the Solidity project and returns the standard JSON output.
pub fn build_solidity_with_options(
    sources: BTreeMap<String, SolcStandardJsonInputSource>,
    libraries: SolcStandardJsonInputSettingsLibraries,
    remappings: BTreeSet<String>,
    optimizer_settings: OptimizerSettings,
    solc_optimizer_enabled: bool,
    suppressed_warnings: Vec<ResolcWarning>,
) -> anyhow::Result<SolcStandardJsonOutput> {
    check_dependencies();
    inkwell::support::enable_llvm_pretty_stack_trace();
    initialize_llvm(PolkaVMTarget::PVM, crate::DEFAULT_EXECUTABLE_NAME, &[]);

    let _ = crate::process::native_process::EXECUTABLE
        .set(PathBuf::from(crate::r#const::DEFAULT_EXECUTABLE_NAME));

    let solc = SolcCompiler::new(SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned())?;
    let solc_version = solc.version()?;

    let mut input = SolcStandardJsonInput::try_from_solidity_sources(
        None,
        sources.clone(),
        libraries.clone(),
        remappings,
        SolcStandardJsonInputSettingsSelection::new_required_for_tests(),
        SolcStandardJsonInputSettingsOptimizer::new(
            solc_optimizer_enabled,
            optimizer_settings
                .middle_end_as_string()
                .chars()
                .last()
                .unwrap(),
            Default::default(),
        ),
        SolcStandardJsonInputSettingsMetadata::default(),
        suppressed_warnings,
        Default::default(),
        Default::default(),
        false,
    )?;

    let mut output = solc.standard_json(&mut input, &mut vec![], None, vec![], None)?;
    if output.has_errors() {
        return Ok(output);
    }
    let debug_config = DebugConfig::new(None, optimizer_settings.middle_end_as_string() != "z");
    let linker_symbols = libraries.as_linker_symbols()?;
    let build = Project::try_from_standard_json_output(
        &mut output,
        libraries,
        &solc_version,
        &debug_config,
    )?
    .compile(
        &mut vec![],
        optimizer_settings,
        MetadataHash::Keccak256,
        &debug_config,
        Default::default(),
        Default::default(),
    )?;
    build.check_errors()?;

    let build = build.link(linker_symbols, &debug_config);
    build.check_errors()?;
    build.write_to_standard_json(&mut output, &solc_version)?;
    output.check_errors()?;

    Ok(output)
}

/// Build a Solidity contract and get the EVM code
pub fn build_solidity_with_options_evm(
    sources: BTreeMap<String, SolcStandardJsonInputSource>,
    libraries: SolcStandardJsonInputSettingsLibraries,
    remappings: BTreeSet<String>,
    solc_optimizer_enabled: bool,
) -> anyhow::Result<BTreeMap<String, (Bytecode, DeployedBytecode)>> {
    check_dependencies();
    inkwell::support::enable_llvm_pretty_stack_trace();
    initialize_llvm(PolkaVMTarget::PVM, crate::DEFAULT_EXECUTABLE_NAME, &[]);
    let _ = crate::process::native_process::EXECUTABLE
        .set(PathBuf::from(crate::r#const::DEFAULT_EXECUTABLE_NAME));

    let solc = SolcCompiler::new(SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned())?;
    let mut input = SolcStandardJsonInput::try_from_solidity_sources(
        None,
        sources.clone(),
        libraries.clone(),
        remappings,
        SolcStandardJsonInputSettingsSelection::new_required_for_tests(),
        SolcStandardJsonInputSettingsOptimizer::new(
            solc_optimizer_enabled,
            Default::default(),
            Default::default(),
        ),
        SolcStandardJsonInputSettingsMetadata::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        false,
    )?;

    let mut contracts = BTreeMap::new();
    for files in solc
        .standard_json(&mut input, &mut vec![], None, vec![], None)?
        .contracts
    {
        for (name, contract) in files.1 {
            if let Some(evm) = contract.evm {
                let (Some(bytecode), Some(deployed_bytecode)) =
                    (evm.bytecode.as_ref(), evm.deployed_bytecode.as_ref())
                else {
                    continue;
                };
                contracts.insert(name.clone(), (bytecode.clone(), deployed_bytecode.clone()));
            }
        }
    }

    Ok(contracts)
}

/// Builds the Solidity project and returns the standard JSON output.
pub fn build_solidity_and_detect_missing_libraries<T: ToString>(
    sources: &[(T, T)],
    libraries: SolcStandardJsonInputSettingsLibraries,
) -> anyhow::Result<SolcStandardJsonOutput> {
    check_dependencies();

    let deployed_libraries = libraries.as_paths();
    let sources = BTreeMap::from_iter(
        sources
            .iter()
            .map(|(path, code)| (path.to_string(), code.to_string().into())),
    );

    inkwell::support::enable_llvm_pretty_stack_trace();
    initialize_llvm(PolkaVMTarget::PVM, crate::DEFAULT_EXECUTABLE_NAME, &[]);
    let _ = crate::process::native_process::EXECUTABLE
        .set(PathBuf::from(crate::r#const::DEFAULT_EXECUTABLE_NAME));

    let solc = SolcCompiler::new(SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned())?;
    let solc_version = solc.version()?;
    let mut input = SolcStandardJsonInput::try_from_solidity_sources(
        None,
        sources.clone(),
        libraries.clone(),
        Default::default(),
        SolcStandardJsonInputSettingsSelection::new_required_for_tests(),
        SolcStandardJsonInputSettingsOptimizer::default(),
        SolcStandardJsonInputSettingsMetadata::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        true,
    )?;

    let mut output = solc.standard_json(&mut input, &mut vec![], None, vec![], None)?;
    if output.has_errors() {
        return Ok(output);
    }

    let project = Project::try_from_standard_json_output(
        &mut output,
        libraries,
        &solc_version,
        &DEBUG_CONFIG,
    )?;

    let missing_libraries = project.get_missing_libraries(&deployed_libraries);
    missing_libraries.write_to_standard_json(&mut output, &solc.version()?);

    Ok(output)
}

/// Checks if the Yul project can be built without errors.
pub fn build_yul<T: ToString + Display>(
    sources: &[(T, T)],
) -> anyhow::Result<BTreeMap<String, Vec<u8>>> {
    check_dependencies();
    inkwell::support::enable_llvm_pretty_stack_trace();
    initialize_llvm(PolkaVMTarget::PVM, crate::DEFAULT_EXECUTABLE_NAME, &[]);

    let _ = crate::process::native_process::EXECUTABLE
        .set(PathBuf::from(crate::r#const::DEFAULT_EXECUTABLE_NAME));

    let mut build = Project::try_from_yul_sources(
        sources
            .iter()
            .map(|(path, source)| {
                (
                    path.to_string(),
                    SolcStandardJsonInputSource::from(source.to_string()),
                )
            })
            .collect(),
        Default::default(),
        None,
        &DEBUG_CONFIG,
    )?
    .compile(
        &mut vec![],
        OptimizerSettings::size(),
        MetadataHash::Keccak256,
        &DEBUG_CONFIG,
        Default::default(),
        Default::default(),
    )?;
    build.take_and_write_warnings();
    build.check_errors()?;

    let mut build = build.link(Default::default(), &DEBUG_CONFIG);
    build.take_and_write_warnings();
    build.check_errors()?;

    Ok(build
        .results
        .into_iter()
        .fold(BTreeMap::new(), |mut init, (path, result)| {
            init.insert(path.to_string(), result.unwrap().build.bytecode);
            init
        }))
}

/// Builds the Yul standard JSON and returns the standard JSON output.
pub fn build_yul_standard_json(
    mut solc_input: SolcStandardJsonInput,
) -> anyhow::Result<SolcStandardJsonOutput> {
    check_dependencies();
    inkwell::support::enable_llvm_pretty_stack_trace();
    initialize_llvm(PolkaVMTarget::PVM, crate::DEFAULT_EXECUTABLE_NAME, &[]);

    let _ = crate::process::native_process::EXECUTABLE
        .set(PathBuf::from(crate::r#const::DEFAULT_EXECUTABLE_NAME));

    let solc = SolcCompiler::new(SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned())?;
    let mut output = solc.validate_yul_standard_json(&mut solc_input, &mut vec![])?;
    if output.has_errors() {
        return Ok(output);
    }
    let build = Project::try_from_yul_sources(
        solc_input.sources,
        Default::default(),
        Some(&mut output),
        &DEBUG_CONFIG,
    )?
    .compile(
        &mut vec![],
        OptimizerSettings::try_from_cli(solc_input.settings.optimizer.mode)?,
        MetadataHash::Keccak256,
        &DEBUG_CONFIG,
        Default::default(),
        Default::default(),
    )?;
    build.check_errors()?;

    let build = build.link(Default::default(), &Default::default());
    build.check_errors()?;
    build.write_to_standard_json(&mut output, &solc.version()?)?;

    Ok(output)
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

/// Compile the blob of `contract_name` found in given `source_code`.
pub fn compile_blob_with_options(
    contract_name: &str,
    source_code: &str,
    solc_optimizer_enabled: bool,
    optimizer_settings: OptimizerSettings,
) -> Vec<u8> {
    let id = CachedBlob {
        contract_name: contract_name.to_owned(),
        opt: optimizer_settings.middle_end_as_string(),
        solc_optimizer_enabled,
        solidity: source_code.to_owned(),
    };

    if let Some(blob) = PVM_BLOB_CACHE.lock().unwrap().get(&id) {
        return blob.clone();
    }

    let file_name = "contract.sol";
    let contracts = build_solidity_with_options(
        BTreeMap::from([(
            file_name.to_owned(),
            SolcStandardJsonInputSource::from(source_code.to_owned()),
        )]),
        Default::default(),
        Default::default(),
        optimizer_settings,
        solc_optimizer_enabled,
        Default::default(),
    )
    .expect("source should compile")
    .contracts;
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
    assert_eq!(&blob[..3], b"PVM");

    PVM_BLOB_CACHE.lock().unwrap().insert(id, blob.clone());

    blob
}

/// Compile the EVM bin-runtime of `contract_name` found in given `source_code`.
/// The `solc` optimizer will be enabled
pub fn compile_evm_bin_runtime(contract_name: &str, source_code: &str) -> Vec<u8> {
    compile_evm(contract_name, source_code, true, true)
}

/// Compile the EVM bin of `contract_name` found in given `source_code`.
pub fn compile_evm_deploy_code(
    contract_name: &str,
    source_code: &str,
    solc_optimizer_enabled: bool,
) -> Vec<u8> {
    compile_evm(contract_name, source_code, solc_optimizer_enabled, false)
}

/// Convert `(path, solidity)` tuples to a standard JSON input source.
pub fn sources<T: ToString>(sources: &[(T, T)]) -> BTreeMap<String, SolcStandardJsonInputSource> {
    BTreeMap::from_iter(
        sources
            .iter()
            .map(|(path, code)| (path.to_string(), code.to_string().into())),
    )
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

/// The internal EVM bytecode compile helper.
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
        BTreeMap::from([(
            file_name.into(),
            SolcStandardJsonInputSource::from(source_code.to_owned()),
        )]),
        Default::default(),
        Default::default(),
        solc_optimizer_enabled,
    )
    .expect("source should compile");
    let object = &contracts
        .get(contract_name)
        .unwrap_or_else(|| panic!("contract '{contract_name}' didn't produce bin-runtime"));
    let code = if runtime {
        object.1.object.as_str()
    } else {
        object.0.object.as_str()
    };
    let blob = hex::decode(code).expect("code shold be hex encoded");

    cache.lock().unwrap().insert(id, blob.clone());

    blob
}

/// Internal helper to compiler `sources` to their optimized Yul IR.
fn generate_yul(
    sources: BTreeMap<String, SolcStandardJsonInputSource>,
    solc_optimizer_enabled: bool,
) -> anyhow::Result<BTreeMap<String, BTreeMap<String, SolcStandardJsonOutputContract>>> {
    check_dependencies();

    let solc = SolcCompiler::new(SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned())?;
    let mut input = SolcStandardJsonInput::try_from_solidity_sources(
        None,
        sources.clone(),
        Default::default(),
        Default::default(),
        SolcStandardJsonInputSettingsSelection::new_required_for_tests(),
        SolcStandardJsonInputSettingsOptimizer::new(
            solc_optimizer_enabled,
            Default::default(),
            Default::default(),
        ),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        false,
    )?;

    let output = solc.standard_json(&mut input, &mut vec![], None, vec![], None)?;
    output.check_errors()?;

    Ok(output.contracts)
}

/// Compiles the Solidity source code into Yul IR and returns
/// the Yul IR code of the contract named `contract_name`.
/// The `solc` optimizer will be enabled.
pub fn compile_to_yul(contract_name: &str, source_code: &str) -> String {
    compile_to_yul_with_options(contract_name, source_code, true)
}

/// Compiles the Solidity source code into Yul IR and returns
/// the Yul IR code of the contract named `contract_name`.
pub fn compile_to_yul_with_options(
    contract_name: &str,
    source_code: &str,
    solc_optimizer_enabled: bool,
) -> String {
    let id = CachedBlob {
        contract_name: contract_name.to_owned(),
        solc_optimizer_enabled,
        solidity: source_code.to_owned(),
        opt: String::new(),
    };

    if let Some(yul) = YUL_IR_CACHE.lock().unwrap().get(&id) {
        return yul.clone();
    }

    let file_name = "contract.sol";
    let sources = BTreeMap::from([(
        file_name.to_owned(),
        SolcStandardJsonInputSource::from(source_code.to_owned()),
    )]);

    generate_yul(sources, solc_optimizer_enabled)
        .expect("source should compile")
        .get(file_name)
        .unwrap_or_else(|| panic!("file `{file_name}` not found in solc output"))
        .get(contract_name)
        .unwrap_or_else(|| panic!("contract `{contract_name}` not found in solc output"))
        .ir_optimized
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::compile_to_yul;

    #[test]
    fn compiles_to_yul() {
        let contract_name = "Dependency";
        let source_code = include_str!("tests/data/solidity/dependency.sol");
        let yul = compile_to_yul(contract_name, source_code);
        assert!(
            yul.contains(&format!("object \"{contract_name}")),
            "the `{contract_name}` contract IR code should contain a Yul object"
        );
    }

    #[test]
    #[should_panic(expected = "contract `Nonexistent` not found in solc output")]
    fn error_nonexistent_contract_in_yul() {
        let contract_name = "Nonexistent";
        let source_code = include_str!("tests/data/solidity/dependency.sol");
        compile_to_yul(contract_name, source_code);
    }
}
