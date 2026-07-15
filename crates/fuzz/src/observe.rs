//! Run compiled bytes on EVM + PVM, capture observations.

use alloy_primitives::{keccak256, Bytes};
use revive_differential::{Evm, RESOURCE_CAP_ERROR};
use revive_runner::{CallResult, Code, OptionalHex, Specs, SpecsAction, TestAddress, ALICE};

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
///
/// Returns `Err` when a run is killed for a runaway execution (the differential
/// runner's output/time cap, e.g. an infinite loop): the EVM side can't be
/// observed, so there is nothing meaningful to compare and the case is skipped.
pub fn observe_evm(deploy_code: Vec<u8>, case: &SolidityCase) -> Result<Outcome, String> {
    let constructor = constructor_calldata(case);
    // geth `evm` expects deploy bytes as hex-ASCII on stdin.
    let deploy_blob = hex::encode(&deploy_code).into_bytes();
    let mut builder = Evm::default().code_blob(deploy_blob).deploy(true);
    if !constructor.is_empty() {
        builder = builder.input(constructor);
    }
    let deploy_log = builder.run();
    if deploy_log.output.error.as_deref() == Some(RESOURCE_CAP_ERROR) {
        return Err("EVM deploy exceeded resource cap".to_owned());
    }
    if deploy_log.output.error.is_some() || deploy_log.account_deployed.is_none() {
        return Ok(Outcome {
            deploy_reverted: true,
            actions: Vec::new(),
        });
    }
    let address = deploy_log.account_deployed.expect("checked above");
    let mut state = deploy_log.state_dump;

    let mut results = Vec::with_capacity(case.actions.len());
    for action in &case.actions {
        let log = Evm::from_genesis(state.clone().into())
            .receiver(address)
            .input(action_calldata(action))
            .run();
        if log.output.error.as_deref() == Some(RESOURCE_CAP_ERROR) {
            return Err("EVM call exceeded resource cap".to_owned());
        }
        results.push(ActionResult {
            reverted: log.output.error.is_some(),
            return_data: log.output.output.to_vec(),
        });
        state = log.state_dump;
    }

    Ok(Outcome {
        deploy_reverted: false,
        actions: results,
    })
}

/// PVM-only resource limits with no EVM equivalent (PolkaVM structural limits +
/// pallet-revive storage-deposit economics): skip, not a compiler divergence.
const PVM_RESOURCE_LIMIT_MARKERS: &[&str] = &[
    "BasicBlockTooLarge",
    "BlobTooLarge",
    "CodeTooLarge",
    "StaticMemoryTooLarge",
    "StorageDepositLimitExhausted",
    "StorageDepositNotEnoughFunds",
];

/// Genesis balance for the origin account.
const ORIGIN_BALANCE: u128 = 1_000_000_000_000;

/// Build the constructor `Instantiate` action for a PVM blob. Uses the runner's
/// default gas/weight (`None`) — runaway inputs are already caught EVM-side,
/// which runs first, so no PVM cap is needed here (a tighter one would revert
/// legit storage-heavy calls that EVM completes, causing false divergences).
fn pvm_instantiate(pvm_blob: Vec<u8>, constructor: Vec<u8>) -> SpecsAction {
    SpecsAction::Instantiate {
        origin: TestAddress::default(),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        code: Code::Bytes(pvm_blob),
        data: constructor,
        salt: OptionalHex::default(),
    }
}

/// Run a PVM action batch on the pallet-revive sim.
fn run_pvm(actions: Vec<SpecsAction>) -> Vec<CallResult> {
    Specs {
        balances: vec![(ALICE, ORIGIN_BALANCE)],
        actions,
        // Read revert flags from CallResult instead of aborting on each revert.
        verify_each_call: false,
        ..Default::default()
    }
    .run()
}

/// The PVM resource-limit marker in a call result, if any (deploy or call).
fn pvm_resource_limit(result: &CallResult) -> Option<&'static str> {
    let debug = format!("{result:?}");
    PVM_RESOURCE_LIMIT_MARKERS
        .iter()
        .copied()
        .find(|marker| debug.contains(marker))
}

/// PVM side of the differential.
///
/// Deploy runs on its own first: pallet-revive panics resolving `Instantiated(0)`
/// for a later call when the deploy failed. Isolating it lets us skip PVM-only
/// structural limits (`Err`) and observe genuine deploy reverts cleanly.
pub fn observe_pvm(pvm_blob: Vec<u8>, case: &SolidityCase) -> Result<Outcome, String> {
    let constructor = constructor_calldata(case).to_vec();

    // Phase 1: deploy alone — a failure here can't hit the dependent-call panic.
    let deploy = run_pvm(vec![pvm_instantiate(pvm_blob.clone(), constructor.clone())]);
    let deploy_result = deploy
        .into_iter()
        .next()
        .expect("instantiate produced no result");

    if deploy_result.did_revert() {
        if let Some(marker) = pvm_resource_limit(&deploy_result) {
            return Err(format!("PVM structural limit at deploy: {marker}"));
        }
        return Ok(Outcome {
            deploy_reverted: true,
            actions: Vec::new(),
        });
    }

    // Phase 2: deploy succeeded, so re-running it with the calls won't panic.
    let mut actions = vec![pvm_instantiate(pvm_blob, constructor)];
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

    let mut results = run_pvm(actions).into_iter();
    let _deploy = results.next(); // already validated in phase 1
    let mut action_results = Vec::with_capacity(case.actions.len());
    for call in results {
        // PVM-only resource limit EVM lacks (e.g. StorageDepositLimitExhausted) — skip.
        if let Some(marker) = pvm_resource_limit(&call) {
            return Err(format!("PVM resource limit on call: {marker}"));
        }
        action_results.push(ActionResult {
            reverted: call.did_revert(),
            return_data: call.output(),
        });
    }

    Ok(Outcome {
        deploy_reverted: false,
        actions: action_results,
    })
}
