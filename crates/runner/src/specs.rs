use serde::{Deserialize, Serialize};

use crate::*;
use revive_solidity::test_utils::*;

/// An action to perform in a contract test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpecsAction {
    /// Instantiate a contract
    Instantiate {
        #[serde(default)]
        origin: TestAccountId,
        #[serde(default)]
        value: Balance,
        #[serde(default)]
        gas_limit: Option<Weight>,
        #[serde(default)]
        storage_deposit_limit: Option<Balance>,
        #[serde(default)]
        code: Code,
        #[serde(default, with = "hex::serde")]
        data: Vec<u8>,
        #[serde(default, with = "hex::serde")]
        salt: Vec<u8>,
    },
    /// Call a contract
    Call {
        #[serde(default)]
        origin: TestAccountId,
        dest: TestAccountId,
        #[serde(default)]
        value: Balance,
        #[serde(default)]
        gas_limit: Option<Weight>,
        #[serde(default)]
        storage_deposit_limit: Option<Balance>,
        #[serde(default, with = "hex::serde")]
        data: Vec<u8>,
    },
    /// Verify the result of the last call, omitting this will simply ensure the last call was successful
    VerifyCall(VerifyCallExpectation),

    /// Verify the balance of an account
    VerifyBalance {
        origin: TestAccountId,
        expected: Balance,
    },
    /// Verify the storage of a contract
    VerifyStorage {
        contract: TestAccountId,
        #[serde(with = "hex::serde")]
        key: Vec<u8>,
        #[serde(default, with = "hex::serde")]
        expected: Vec<u8>,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum TestAccountId {
    /// The ALICE account
    #[default]
    Alice,
    /// The BOB account
    Bob,
    /// The CHARLIE account
    Charlie,
    /// AccountID that was created during the nth call in this run.
    Instantiated(u32),
    /// Arbitrary AccountID
    AccountId(AccountId),
}

/// Specs for a contract test
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Specs {
    /// List of endowments at genesis
    pub balances: Vec<(AccountId, Balance)>,
    /// List of actions to perform
    pub actions: Vec<SpecsAction>,
}

impl Default for Specs {
    fn default() -> Self {
        Self {
            balances: vec![(ALICE, 1_000_000_000)],
            actions: Default::default(),
        }
    }
}

impl Specs {
    /// Get the list of actions to perform
    /// A default [`SpecAction::VerifyCall`] is injected after each Instantiate or Call action when
    /// missing
    pub fn actions(&self) -> Vec<SpecsAction> {
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

    pub fn replace_empty_code(&mut self, contract_name: &str, contract_source: &str) {
        for action in self.actions.iter_mut() {
            let SpecsAction::Instantiate { code, .. } = action else {
                continue;
            };

            match code {
                Code::Bytes(bytes) if bytes.is_empty() => {
                    *bytes = compile_blob(contract_name, contract_source)
                }
                Code::Solidity {
                    path,
                    solc_optimizer,
                    pipeline,
                    contract,
                } if path.is_none() => {
                    *code = Code::Bytes(compile_blob_with_options(
                        contract.as_str(),
                        contract_source,
                        solc_optimizer.unwrap_or(true),
                        pipeline.unwrap_or(revive_solidity::SolcPipeline::Yul),
                    ));
                }
                _ => continue,
            }
        }
    }
}
