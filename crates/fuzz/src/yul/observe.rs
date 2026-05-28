//! Execute Yul-compiled artifacts on EVM and PVM, returning the
//! shared [`Outcome`] shape.

use alloy_primitives::Bytes;
use revive_differential::Evm;
use revive_runner::{Code, OptionalHex, Specs, SpecsAction, TestAddress, ALICE};

use crate::observe::{ActionResult, Outcome};

/// Deploy with empty ctor data, then replay calls on geth's `evm`,
/// threading state across via `Evm::from_genesis`.
pub fn observe_evm_yul(deploy_code: Vec<u8>, action_calldata: &[Vec<u8>]) -> Outcome {
    // geth `evm` expects deploy bytes as hex-ASCII on stdin.
    let deploy_blob = hex::encode(&deploy_code).into_bytes();
    let deploy_log = Evm::default()
        .code_blob(deploy_blob)
        .deploy(true)
        .run();
    if deploy_log.output.error.is_some() || deploy_log.account_deployed.is_none() {
        return Outcome { deploy_reverted: true, actions: vec![] };
    }
    let address = deploy_log.account_deployed.expect("checked above");
    let mut state = deploy_log.state_dump;

    let mut results = Vec::with_capacity(action_calldata.len());
    for calldata in action_calldata {
        let log = Evm::from_genesis(state.clone().into())
            .receiver(address)
            .input(Bytes::from(calldata.clone()))
            .run();
        results.push(ActionResult {
            reverted: log.output.error.is_some(),
            return_data: log.output.output.to_vec(),
        });
        state = log.state_dump;
    }

    Outcome { deploy_reverted: false, actions: results }
}

/// Same on pallet-revive sim. Ctor data is empty — initial storage
/// is baked into the generated Yul (see `generator.rs` module docs).
pub fn observe_pvm_yul(pvm_blob: Vec<u8>, action_calldata: &[Vec<u8>]) -> Outcome {
    let mut actions = vec![SpecsAction::Instantiate {
        origin: TestAddress::default(),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        code: Code::Bytes(pvm_blob),
        data: vec![],
        salt: OptionalHex::default(),
    }];
    for calldata in action_calldata {
        actions.push(SpecsAction::Call {
            origin: TestAddress::default(),
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: calldata.clone(),
        });
    }

    let mut results = Specs {
        balances: vec![(ALICE, 1_000_000_000_000)],
        actions,
        // See `observe_pvm` in the parent module: VerifyCall would
        // abort on every generated revert.
        verify_each_call: false,
        ..Default::default()
    }
    .run()
    .into_iter();

    let deploy_result = results.next().expect("instantiate produced no result");
    if deploy_result.did_revert() {
        return Outcome { deploy_reverted: true, actions: vec![] };
    }

    let action_results = results
        .map(|call| ActionResult {
            reverted: call.did_revert(),
            return_data: call.output(),
        })
        .collect();

    Outcome { deploy_reverted: false, actions: action_results }
}
