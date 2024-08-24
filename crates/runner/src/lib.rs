//! Experimental test runner for testing [pallet-revive](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/revive) contracts.
//! The crate exposes a single function [`run_tests`] that takes a [`Specs`] that defines in a declarative way:
//! - The Genesis configuration
//! - A list of [`SpecsAction`] that will be executed in order.
//!
//! ## Example
//! ```rust
//! use revive_runner::*;
//! use SpecsAction::*;
//! run_test(Specs {
//!     balances: vec![(ALICE, 1_000_000_000)],
//!     actions: vec![Instantiate {
//!         origin: ALICE,
//!         value: 0,
//!         gas_limit: Some(GAS_LIMIT),
//!         storage_deposit_limit: Some(DEPOSIT_LIMIT),
//!         code: Code::Bytes(include_bytes!("../fixtures/Baseline.pvm").to_vec()),
//!         data: vec![],
//!         salt: vec![],
//!     }],
//! })
//! ```

use polkadot_sdk::*;
use polkadot_sdk::{
    pallet_revive::{CollectEvents, ContractExecResult, ContractInstantiateResult, DebugInfo},
    polkadot_runtime_common::BuildStorage,
    polkadot_sdk_frame::testing_prelude::*,
    sp_keystore::{testing::MemoryKeystore, KeystoreExt},
    sp_runtime::AccountId32,
};
use serde::{Deserialize, Serialize};

mod runtime;
use crate::runtime::*;

pub const ALICE: AccountId32 = AccountId32::new([1u8; 32]);
pub const BOB: AccountId32 = AccountId32::new([2u8; 32]);
pub const CHARLIE: AccountId32 = AccountId32::new([3u8; 32]);

/// Externalities builder
#[derive(Default)]
pub struct ExtBuilder {
    /// List of endowments at genesis
    balance_genesis_config: Vec<(AccountId, Balance)>,
}

impl ExtBuilder {
    /// Set the balance of an account at genesis
    fn balance_genesis_config(mut self, value: Vec<(AccountId, Balance)>) -> Self {
        self.balance_genesis_config = value;
        self
    }

    /// Build the externalities
    pub fn build(self) -> sp_io::TestExternalities {
        sp_tracing::try_init_simple();
        let mut t = frame_system::GenesisConfig::<Runtime>::default()
            .build_storage()
            .unwrap();
        pallet_balances::GenesisConfig::<Runtime> {
            balances: self.balance_genesis_config,
        }
        .assimilate_storage(&mut t)
        .unwrap();
        let mut ext = sp_io::TestExternalities::new(t);
        ext.register_extension(KeystoreExt::new(MemoryKeystore::new()));
        ext.execute_with(|| System::set_block_number(1));

        ext
    }
}

/// Default gas limit
pub const GAS_LIMIT: Weight = Weight::from_parts(100_000_000_000, 3 * 1024 * 1024);

/// Default deposit limit
pub const DEPOSIT_LIMIT: Balance = 10_000_000;

/// Expectation for a call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyCallExpectation {
    /// When provided, the expected gas consumed
    gas_consumed: Option<Weight>,
    /// When provided, the expected output
    output: Option<Vec<u8>>,
    ///Expected call result
    success: bool,
}

impl Default for VerifyCallExpectation {
    fn default() -> Self {
        Self {
            gas_consumed: None,
            output: None,
            success: true,
        }
    }
}

impl VerifyCallExpectation {
    /// Verify that the expectations are met
    fn verify(self, result: CallResult) {
        dbg!(&result);
        assert_eq!(self.success, result.is_ok());
        if let Some(gas_consumed) = self.gas_consumed {
            assert_eq!(gas_consumed, result.gas_consumed());
        }
        if let Some(output) = self.output {
            assert_eq!(output, result.output());
        }
    }
}

/// Result of a call
#[derive(Debug)]
enum CallResult {
    Exec(ContractExecResult<Balance, EventRecord>),
    Instantiate(ContractInstantiateResult<AccountId, Balance, EventRecord>),
}

impl CallResult {
    /// Check if the call was successful
    fn is_ok(&self) -> bool {
        match self {
            Self::Exec(res) => res.result.is_ok(),
            Self::Instantiate(res) => res.result.is_ok(),
        }
    }
    /// Get the output of the call
    fn output(&self) -> Vec<u8> {
        match self {
            Self::Exec(res) => res
                .result
                .as_ref()
                .map(|r| r.data.clone())
                .unwrap_or_default(),
            Self::Instantiate(res) => res
                .result
                .as_ref()
                .map(|r| r.result.data.clone())
                .unwrap_or_default(),
        }
    }
    /// Get the gas consumed by the call
    fn gas_consumed(&self) -> Weight {
        match self {
            Self::Exec(res) => res.gas_consumed,
            Self::Instantiate(res) => res.gas_consumed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Code {
    /// Compile a single solidity source and use the blob of `contract`
    Solidity {
        path: std::path::PathBuf,
        contract: String,
    },
    /// Read the contract blob from disk
    Path(std::path::PathBuf),
    /// A contract blob
    Bytes(Vec<u8>),
    /// Pre-existing contract hash
    Hash(Hash),
}

impl From<Code> for pallet_revive::Code<Hash> {
    fn from(val: Code) -> Self {
        match val {
            Code::Solidity { path, contract } => {
                pallet_revive::Code::Upload(revive_solidity::test_utils::compile_blob(
                    contract.as_str(),
                    std::fs::read_to_string(path).unwrap().as_str(),
                ))
            }
            Code::Path(path) => pallet_revive::Code::Upload(std::fs::read(path).unwrap()),
            Code::Bytes(bytes) => pallet_revive::Code::Upload(bytes),
            Code::Hash(hash) => pallet_revive::Code::Existing(hash),
        }
    }
}

/// An action to perform in a contract test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpecsAction {
    /// Instantiate a contract
    Instantiate {
        origin: AccountId,
        #[serde(default)]
        value: Balance,
        #[serde(default)]
        gas_limit: Option<Weight>,
        #[serde(default)]
        storage_deposit_limit: Option<Balance>,
        code: Code,
        #[serde(default)]
        data: Vec<u8>,
        #[serde(default)]
        salt: Vec<u8>,
    },
    /// Call a contract
    Call {
        origin: AccountId,
        dest: AccountId,
        #[serde(default)]
        value: Balance,
        #[serde(default)]
        gas_limit: Option<Weight>,
        #[serde(default)]
        storage_deposit_limit: Option<Balance>,
        #[serde(default)]
        data: Vec<u8>,
    },
    /// Verify the result of the last call, omitting this will simply ensure the last call was successful
    VerifyCall(VerifyCallExpectation),

    /// Verify the balance of an account
    VerifyBalance {
        origin: AccountId,
        expected: Balance,
    },
    /// Verify the storage of a contract
    VerifyStorage {
        contract: AccountId,
        key: Vec<u8>,
        expected: Option<Vec<u8>>,
    },
}

/// Specs for a contract test
#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Specs {
    /// List of endowments at genesis
    pub balances: Vec<(AccountId, Balance)>,
    /// List of actions to perform
    pub actions: Vec<SpecsAction>,
}

impl Specs {
    /// Get the list of actions to perform
    /// A default [`SpecAction::VerifyCall`] is injected after each Instantiate or Call action when
    /// missing
    fn actions(&self) -> Vec<SpecsAction> {
        self.actions
            .iter()
            .enumerate()
            .flat_map(|(index, item)| {
                let next_item = self.actions.get(index + 1);
                if matches!(
                    item,
                    SpecsAction::Instantiate { .. } | SpecsAction::Call { .. }
                ) && !matches!(next_item, Some(SpecsAction::VerifyCall(_)))
                {
                    return vec![
                        item.clone(),
                        SpecsAction::VerifyCall(VerifyCallExpectation::default()),
                    ];
                }
                vec![item.clone()]
            })
            .collect()
    }
}

/// Run a contract test
/// The test takes a [`Specs`] and executes the actions in order
pub fn run_test(specs: Specs) {
    ExtBuilder::default()
        .balance_genesis_config(specs.balances.clone())
        .build()
        .execute_with(|| {
            use SpecsAction::*;

            let mut res: Option<CallResult> = None;
            let actions = specs.actions();

            for action in actions {
                match action {
                    Instantiate {
                        origin,
                        value,
                        gas_limit,
                        storage_deposit_limit,
                        code,
                        data,
                        salt,
                    } => {
                        res = Some(CallResult::Instantiate(Contracts::bare_instantiate(
                            RuntimeOrigin::signed(origin),
                            value,
                            gas_limit.unwrap_or(GAS_LIMIT),
                            storage_deposit_limit.unwrap_or(DEPOSIT_LIMIT),
                            code.into(),
                            data,
                            salt,
                            DebugInfo::Skip,
                            CollectEvents::Skip,
                        )));
                    }
                    Call {
                        origin,
                        dest,
                        value,
                        gas_limit,
                        storage_deposit_limit,
                        data,
                    } => {
                        res = Some(CallResult::Exec(Contracts::bare_call(
                            RuntimeOrigin::signed(origin),
                            dest,
                            value,
                            gas_limit.unwrap_or(GAS_LIMIT),
                            storage_deposit_limit.unwrap_or(DEPOSIT_LIMIT),
                            data,
                            DebugInfo::Skip,
                            CollectEvents::Skip,
                        )));
                    }
                    VerifyCall(expectation) => {
                        if let Some(res) = res.take() {
                            expectation.verify(res);
                        } else {
                            panic!("No call to verify");
                        }
                    }
                    VerifyBalance { origin, expected } => {
                        assert_eq!(Balances::free_balance(&origin), expected);
                    }
                    VerifyStorage {
                        contract,
                        key,
                        expected,
                    } => {
                        let Ok(storage) = Contracts::get_storage(contract, key) else {
                            panic!("Error reading storage");
                        };
                        assert_eq!(storage, expected);
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instantiate_works() {
        use SpecsAction::*;
        run_test(Specs {
            balances: vec![(ALICE, 1_000_000_000)],
            actions: vec![Instantiate {
                origin: ALICE,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: Some(DEPOSIT_LIMIT),
                code: Code::Bytes(include_bytes!("../fixtures/Baseline.pvm").to_vec()),
                data: vec![],
                salt: vec![],
            }],
        })
    }

    #[test]
    fn instantiate_with_json() {
        let specs = serde_json::from_str::<Specs>(
            r#"
        {
        "balances": [
            [ "5C62Ck4UrFPiBtoCmeSrgF7x9yv9mn38446dhCpsi2mLHiFT", 1000000000 ]
        ],
        "actions": [
            {
                "Instantiate": {
                    "origin": "5C62Ck4UrFPiBtoCmeSrgF7x9yv9mn38446dhCpsi2mLHiFT",
                    "value": 0,
                    "code": {
                        "Path": "fixtures/Baseline.pvm"
                    }
                }
            }
        ]
        }
    "#,
        )
        .unwrap();
        run_test(specs);
    }
}
