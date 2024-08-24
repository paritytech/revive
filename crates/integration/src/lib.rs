use alloy_primitives::{Address, U256};
use cases::Contract;
use mock_runtime::{CallOutput, State};

use crate::mock_runtime::{Event, ReturnFlags};

pub mod cases;
pub mod mock_runtime;

#[cfg(test)]
mod tests;

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
