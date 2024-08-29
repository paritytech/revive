//! Experimental test runner for testing [pallet-revive](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/revive) contracts.
//! The crate exposes a single function [`run_tests`] that takes a [`Specs`] that defines in a declarative way:
//! - The Genesis configuration
//! - A list of [`SpecsAction`] that will be executed in order.
//!
//! ## Example
//! ```rust
//! use revive_runner::*;
//! use SpecsAction::*;
//! Specs {
//!     differential: false,
//!     balances: vec![(ALICE, 1_000_000_000)],
//!     actions: vec![Instantiate {
//!         origin: TestAccountId::Alice,
//!         value: 0,
//!         gas_limit: Some(GAS_LIMIT),
//!         storage_deposit_limit: Some(DEPOSIT_LIMIT),
//!         code: Code::Bytes(include_bytes!("../fixtures/Baseline.pvm").to_vec()),
//!         data: vec![],
//!         salt: vec![],
//!     }],
//! }
//! .run();
//! ```

use polkadot_sdk::*;
use polkadot_sdk::{
    pallet_revive::{CollectEvents, ContractExecResult, ContractInstantiateResult, DebugInfo},
    polkadot_runtime_common::BuildStorage,
    polkadot_sdk_frame::testing_prelude::*,
    sp_keystore::{testing::MemoryKeystore, KeystoreExt},
};
use serde::{Deserialize, Serialize};

mod runtime;
mod specs;

use crate::runtime::*;
pub use crate::specs::*;

pub const ALICE: AccountId = AccountId::new([1u8; 32]);
pub const BOB: AccountId = AccountId::new([2u8; 32]);
pub const CHARLIE: AccountId = AccountId::new([3u8; 32]);

const SPEC_MARKER_BEGIN: &str = "/* runner.json";
const SPEC_MARKER_END: &str = "*/";

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
    pub gas_consumed: Option<Weight>,
    /// When provided, the expected output
    #[serde(default, with = "hex::serde")]
    pub output: Vec<u8>,
    ///Expected call result
    pub success: bool,
}

impl Default for VerifyCallExpectation {
    fn default() -> Self {
        Self {
            gas_consumed: None,
            output: vec![],
            success: true,
        }
    }
}

impl VerifyCallExpectation {
    /// Verify that the expectations are met
    fn verify(self, result: &CallResult) {
        assert_eq!(
            self.success,
            result.is_ok(),
            "contract execution result mismatch: {result:?}"
        );
        if let Some(gas_consumed) = self.gas_consumed {
            assert_eq!(gas_consumed, result.gas_consumed());
        }
        assert_eq!(self.output, result.output());
    }
}

/// Result of a call
#[derive(Clone, Debug)]
pub enum CallResult {
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
        path: Option<std::path::PathBuf>,
        solc_optimizer: Option<bool>,
        pipeline: Option<revive_solidity::SolcPipeline>,
        contract: String,
    },
    /// Read the contract blob from disk
    Path(std::path::PathBuf),
    /// A contract blob
    Bytes(Vec<u8>),
    /// Pre-existing contract hash
    Hash(Hash),
}

impl Default for Code {
    fn default() -> Self {
        Self::Bytes(vec![])
    }
}

impl From<Code> for pallet_revive::Code<Hash> {
    fn from(val: Code) -> Self {
        match val {
            Code::Solidity {
                path,
                contract,
                solc_optimizer,
                pipeline,
            } => {
                let Some(path) = path else {
                    panic!("Solidity source of contract '{contract}' missing path");
                };
                let Ok(source_code) = std::fs::read_to_string(&path) else {
                    panic!("Failed to reead source code from {}", path.display());
                };
                pallet_revive::Code::Upload(revive_solidity::test_utils::compile_blob_with_options(
                    &contract,
                    &source_code,
                    solc_optimizer.unwrap_or(true),
                    pipeline.unwrap_or(revive_solidity::SolcPipeline::Yul),
                ))
            }
            Code::Path(path) => pallet_revive::Code::Upload(std::fs::read(path).unwrap()),
            Code::Bytes(bytes) => pallet_revive::Code::Upload(bytes),
            Code::Hash(hash) => pallet_revive::Code::Existing(hash),
        }
    }
}

pub fn specs_from_comment(contract_name: &str, path: &str) -> Vec<Specs> {
    let solidity = match std::fs::read_to_string(path) {
        Err(err) => panic!("unable to read {path}: {err}"),
        Ok(solidity) => solidity,
    };
    let mut json_string = String::with_capacity(solidity.len());
    let mut is_reading = false;
    let mut specs = Vec::new();

    for line in solidity.lines() {
        if line.starts_with(SPEC_MARKER_BEGIN) {
            is_reading = true;
            continue;
        }
        if line.starts_with(SPEC_MARKER_END) {
            match serde_json::from_str::<Specs>(&json_string) {
                Ok(mut spec) => {
                    spec.replace_empty_code(contract_name, path);
                    specs.push(spec);
                }
                Err(e) => panic!("invalid spec JSON: {e}"),
            }
            is_reading = false;
            json_string.clear();
            continue;
        }
        if is_reading {
            json_string.push_str(line)
        }
    }

    assert!(!specs.is_empty(), "source does not contain any test spec");

    specs
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn instantiate_works() {
        use specs::SpecsAction::*;
        let specs = Specs {
            differential: false,
            balances: vec![(ALICE, 1_000_000_000)],
            actions: vec![Instantiate {
                origin: TestAccountId::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: Some(DEPOSIT_LIMIT),
                code: Code::Bytes(include_bytes!("../fixtures/Baseline.pvm").to_vec()),
                data: vec![],
                salt: vec![],
            }],
        };
        specs.run();
    }

    #[test]
    fn instantiate_with_json() {
        serde_json::from_str::<Specs>(
            r#"
        {
        "balances": [
            [ "5C62Ck4UrFPiBtoCmeSrgF7x9yv9mn38446dhCpsi2mLHiFT", 1000000000 ]
        ],
        "actions": [
            {
                "Instantiate": {
                    "origin": "Alice",
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
        .unwrap()
        .run();
    }
}
