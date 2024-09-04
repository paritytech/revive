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
//!         origin: TestAddress::Alice,
//!         value: 0,
//!         gas_limit: Some(GAS_LIMIT),
//!         storage_deposit_limit: Some(DEPOSIT_LIMIT),
//!         code: Code::Bytes(include_bytes!("../fixtures/Baseline.pvm").to_vec()),
//!         data: vec![],
//!         salt: Default::default(),
//!     }],
//! }
//! .run();
//! ```

use std::time::Duration;

use hex::{FromHex, ToHex};
use pallet_revive::AddressMapper;
use polkadot_sdk::*;
use polkadot_sdk::{
    pallet_revive::{CollectEvents, ContractExecResult, ContractInstantiateResult, DebugInfo},
    polkadot_runtime_common::BuildStorage,
    polkadot_sdk_frame::testing_prelude::*,
    sp_core::H160,
    sp_keystore::{testing::MemoryKeystore, KeystoreExt},
    sp_runtime::AccountId32,
};
use serde::{Deserialize, Serialize};

use crate::runtime::*;
pub use crate::specs::*;

mod runtime;
mod specs;

/// The alice test account
pub const ALICE: H160 = H160([1u8; 20]);
/// The bob test account
pub const BOB: H160 = H160([2u8; 20]);
/// The charlie test account
pub const CHARLIE: H160 = H160([3u8; 20]);
/// Default gas limit
pub const GAS_LIMIT: Weight = Weight::from_parts(100_000_000_000, 3 * 1024 * 1024);
/// Default deposit limit
pub const DEPOSIT_LIMIT: Balance = 10_000_000;

/// Externalities builder
#[derive(Default)]
pub struct ExtBuilder {
    /// List of endowments at genesis
    balance_genesis_config: Vec<(AccountId32, Balance)>,
}

impl ExtBuilder {
    /// Set the balance of an account at genesis
    fn balance_genesis_config(self, value: Vec<(H160, Balance)>) -> Self {
        Self {
            balance_genesis_config: value
                .iter()
                .map(|(address, balance)| (AccountId::to_account_id(address), *balance))
                .collect(),
        }
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

/// Expectation for a call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyCallExpectation {
    /// When provided, the expected gas consumed
    pub gas_consumed: Option<Weight>,
    /// When provided, the expected output
    #[serde(default, with = "hex")]
    pub output: OptionalHex<Vec<u8>>,
    ///Expected call result
    pub success: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OptionalHex<T>(Option<T>);

impl<I: FromHex + AsRef<[u8]>> FromHex for OptionalHex<I> {
    type Error = <I as FromHex>::Error;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        let value = I::from_hex(hex)?;
        Ok(Self(Some(value)))
    }
}

impl<I: AsRef<[u8]>> ToHex for &OptionalHex<I> {
    fn encode_hex<T: std::iter::FromIterator<char>>(&self) -> T {
        match self.0.as_ref() {
            None => T::from_iter("".chars()),
            Some(data) => I::encode_hex::<T>(data),
        }
    }

    fn encode_hex_upper<T: std::iter::FromIterator<char>>(&self) -> T {
        match self.0.as_ref() {
            None => T::from_iter("".chars()),
            Some(data) => I::encode_hex_upper(data),
        }
    }
}

impl<T: AsRef<[u8]>> From<T> for OptionalHex<T> {
    fn from(value: T) -> Self {
        if value.as_ref().is_empty() {
            OptionalHex(None)
        } else {
            OptionalHex(Some(value))
        }
    }
}

impl Default for VerifyCallExpectation {
    fn default() -> Self {
        Self {
            gas_consumed: None,
            output: OptionalHex(None),
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

        if let OptionalHex(Some(data)) = self.output {
            assert_eq!(data, result.output());
        }
    }
}

/// Result of a call
#[derive(Clone, Debug)]
pub enum CallResult {
    Exec {
        result: ContractExecResult<Balance, EventRecord>,
        wall_time: Duration,
    },
    Instantiate {
        result: ContractInstantiateResult<Balance, EventRecord>,
        wall_time: Duration,
    },
}

impl CallResult {
    /// Check if the call was successful
    fn is_ok(&self) -> bool {
        match self {
            Self::Exec { result, .. } => result.result.is_ok(),
            Self::Instantiate { result, .. } => result.result.is_ok(),
        }
    }
    /// Get the output of the call
    fn output(&self) -> Vec<u8> {
        match self {
            Self::Exec { result, .. } => result
                .result
                .as_ref()
                .map(|r| r.data.clone())
                .unwrap_or_default(),
            Self::Instantiate { result, .. } => result
                .result
                .as_ref()
                .map(|r| r.result.data.clone())
                .unwrap_or_default(),
        }
    }
    /// Get the gas consumed by the call
    fn gas_consumed(&self) -> Weight {
        match self {
            Self::Exec { result, .. } => result.gas_consumed,
            Self::Instantiate { result, .. } => result.gas_consumed,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

impl From<Code> for pallet_revive::Code {
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
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: Some(DEPOSIT_LIMIT),
                code: Code::Bytes(include_bytes!("../fixtures/Baseline.pvm").to_vec()),
                data: vec![],
                salt: OptionalHex::default(),
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
            [ "0101010101010101010101010101010101010101", 1000000000 ]
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
