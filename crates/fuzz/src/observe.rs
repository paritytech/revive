//! Run compiled bytes on EVM + PVM, capture observations.

use alloy_primitives::{keccak256, Bytes};
use revive_differential::Evm;
use revive_runner::{Code, OptionalHex, Specs, SpecsAction, TestAddress, ALICE};

use crate::generator::{Action, SolidityCase};

/// What the differential reads back. No `final_storage` — pallet-
/// revive's storage is only queryable inside its `ExtBuilder`
/// closure. Storage divergence surfaces through later action
/// return-data, provided each template's `fn_0` reads its state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Constructor reverted.
    pub deploy_reverted: bool,
    /// One entry per `case.actions` (empty if the deploy reverted).
    pub actions: Vec<ActionResult>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActionResult {
    pub reverted: bool,
    pub return_data: Vec<u8>,
}

/// Constructor calldata = concatenation of 32-byte arguments.
pub fn constructor_calldata(case: &SolidityCase) -> Bytes {
    let mut buf = Vec::with_capacity(case.constructor_args.len() * 32);
    for arg in &case.constructor_args {
        buf.extend_from_slice(arg);
    }
    buf.into()
}

/// `selector(fn_0(uint256)) || arg32`. Every template exposes `fn_0`.
pub fn action_calldata(action: &Action) -> Bytes {
    let selector = &keccak256(b"fn_0(uint256)")[..4];
    let mut buf = Vec::with_capacity(36);
    buf.extend_from_slice(selector);
    buf.extend_from_slice(&action.argument);
    buf.into()
}

/// Deploy + replay on geth's `evm`; state threaded via `from_genesis`.
pub fn observe_evm(deploy_code: Vec<u8>, case: &SolidityCase) -> Outcome {
    let constructor = constructor_calldata(case);
    // geth `evm` expects deploy bytes as hex-ASCII on stdin.
    let deploy_blob = hex::encode(&deploy_code).into_bytes();
    let mut builder = Evm::default().code_blob(deploy_blob).deploy(true);
    if !constructor.is_empty() {
        builder = builder.input(constructor);
    }
    let deploy_log = builder.run();
    if deploy_log.output.error.is_some() || deploy_log.account_deployed.is_none() {
        return Outcome {
            deploy_reverted: true,
            actions: vec![],
        };
    }
    let address = deploy_log.account_deployed.expect("checked above");
    let mut state = deploy_log.state_dump;

    let mut results = Vec::with_capacity(case.actions.len());
    for action in &case.actions {
        let log = Evm::from_genesis(state.clone().into())
            .receiver(address)
            .input(action_calldata(action))
            .run();
        results.push(ActionResult {
            reverted: log.output.error.is_some(),
            return_data: log.output.output.to_vec(),
        });
        state = log.state_dump;
    }

    Outcome {
        deploy_reverted: false,
        actions: results,
    }
}

/// Same on `revive-runner`'s pallet-revive sim. We orchestrate the
/// EVM side ourselves, so `differential: false` here.
pub fn observe_pvm(pvm_blob: Vec<u8>, case: &SolidityCase) -> Outcome {
    let constructor = constructor_calldata(case).to_vec();
    let mut actions = vec![SpecsAction::Instantiate {
        origin: TestAddress::default(),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        code: Code::Bytes(pvm_blob),
        data: constructor,
        salt: OptionalHex::default(),
    }];
    for action in &case.actions {
        actions.push(SpecsAction::Call {
            origin: TestAddress::default(),
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: action_calldata(action).to_vec(),
        });
    }

    let mut results = Specs {
        balances: vec![(ALICE, 1_000_000_000_000)],
        actions,
        // Default VerifyCall(success: true) would abort on every
        // revert; we read revert flags from CallResult directly.
        verify_each_call: false,
        ..Default::default()
    }
    .run()
    .into_iter();

    let deploy_result = results.next().expect("instantiate produced no result");
    if deploy_result.did_revert() {
        return Outcome {
            deploy_reverted: true,
            actions: vec![],
        };
    }

    let action_results = results
        .map(|call| ActionResult {
            reverted: call.did_revert(),
            return_data: call.output(),
        })
        .collect();

    Outcome {
        deploy_reverted: false,
        actions: action_results,
    }
}
