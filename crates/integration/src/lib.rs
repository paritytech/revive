use alloy_primitives::{Address, U256};
use cases::Contract;
use mock_runtime::{CallOutput, State};

use crate::mock_runtime::{Event, ReturnFlags};

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
        .bytecode
        .as_ref()
        .expect("source should produce assembly text")
        .object
        .as_str();

    hex::decode(bytecode).expect("hex encoding should always be valid")
}

pub fn assert_success(contract: &Contract, differential: bool) -> (State, CallOutput) {
    let (state, output) = contract.execute();
    assert_eq!(output.flags, ReturnFlags::Success);

    if differential {
        let evm =
            revive_differential::prepare(contract.evm_runtime.clone(), contract.calldata.clone());
        let (evm_output, evm_log) = revive_differential::execute(evm);

        assert_eq!(output.data.clone(), evm_output);
        assert_eq!(output.events.len(), evm_log.len());
        assert_eq!(
            output.events,
            evm_log
                .iter()
                .map(|log| Event {
                    address: Address::from_slice(log.address.as_bytes()),
                    data: log.data.clone(),
                    topics: log
                        .topics
                        .iter()
                        .map(|topic| U256::from_be_bytes(topic.0))
                        .collect(),
                })
                .collect::<Vec<_>>()
        );
    }

    (state, output)
}
