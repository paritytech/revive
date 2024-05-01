use cases::Contract;
use mock_runtime::State;

pub mod cases;
pub mod mock_runtime;

#[cfg(test)]
mod tests;

/// Compile the blob of `contract_name` found in given `source_code`.
/// The `solc` optimizer will be enabled
pub fn compile_blob(contract_name: &str, source_code: &str) -> Vec<u8> {
    compile_blob_with_options(
        contract_name,
        source_code,
        true,
        revive_solidity::SolcPipeline::Yul,
    )
}

/// Compile the EVM bin-runtime of `contract_name` found in given `source_code`.
/// The `solc` optimizer will be enabled
pub fn compile_evm_bin_runtime(contract_name: &str, source_code: &str) -> Vec<u8> {
    let file_name = "contract.sol";

    let contracts = revive_solidity::test_utils::build_solidity_with_options_evm(
        [(file_name.into(), source_code.into())].into(),
        Default::default(),
        None,
        revive_solidity::SolcPipeline::Yul,
        true,
    )
    .expect("source should compile");
    let bin_runtime = &contracts
        .get(contract_name)
        .unwrap_or_else(|| panic!("contract '{}' didn't produce bin-runtime", contract_name))
        .object;

    hex::decode(bin_runtime).expect("bin-runtime shold be hex encoded")
}

/// Compile the blob of `contract_name` found in given `source_code`.
pub fn compile_blob_with_options(
    contract_name: &str,
    source_code: &str,
    solc_optimizer_enabled: bool,
    pipeline: revive_solidity::SolcPipeline,
) -> Vec<u8> {
    let file_name = "contract.sol";

    let contracts = revive_solidity::test_utils::build_solidity_with_options(
        [(file_name.into(), source_code.into())].into(),
        Default::default(),
        None,
        pipeline,
        revive_llvm_context::OptimizerSettings::cycles(),
        solc_optimizer_enabled,
    )
    .expect("source should compile")
    .contracts
    .expect("source should contain at least one contract");

    let bytecode = contracts[file_name][contract_name]
        .evm
        .as_ref()
        .expect("source should produce EVM output")
        .assembly_text
        .as_ref()
        .expect("source should produce assembly text");

    hex::decode(bytecode).expect("hex encoding should always be valid")
}

pub fn assert_success(contract: Contract, differential: bool) -> State {
    let (mut instance, export) = mock_runtime::prepare(&contract.pvm_runtime, None);
    let state = mock_runtime::call(State::new(contract.calldata.clone()), &mut instance, export);
    assert_eq!(state.output.flags, 0);

    if differential {
        let evm = revive_differential::prepare(contract.evm_runtime, contract.calldata);
        assert_eq!(state.output.data.clone(), revive_differential::execute(evm));
    }

    state
}
