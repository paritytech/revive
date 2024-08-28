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

impl TestAccountId {
    fn to_account_id(&self, results: &[CallResult]) -> AccountId {
        match self {
            TestAccountId::Alice => ALICE,
            TestAccountId::Bob => BOB,
            TestAccountId::Charlie => CHARLIE,
            TestAccountId::AccountId(account_id) => account_id.clone(),
            TestAccountId::Instantiated(n) => match results
                .get(*n as usize)
                .expect("should provide valid index into call results")
            {
                CallResult::Exec(_) => panic!("call #{n} should be an instantiation"),
                CallResult::Instantiate(res) => res
                    .result
                    .as_ref()
                    .expect("call #{n} reverted")
                    .account_id
                    .clone(),
            },
        }
    }
}

/// Specs for a contract test
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Specs {
    /// Interpret EVM bytecode and assert output, storage and events
    #[serde(default)]
    pub differential: bool,
    /// List of endowments at genesis
    pub balances: Vec<(AccountId, Balance)>,
    /// List of actions to perform
    pub actions: Vec<SpecsAction>,
}

impl Default for Specs {
    fn default() -> Self {
        Self {
            differential: false,
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

    /// Helper to allow not specifying the code bytes or path directly in the runner.json
    /// - Replace `Code::Bytes(bytes)` if `bytes` are empty: read `contract_file`
    /// - Replace `Code::Solidity{ path, ..}` if `path` is not provided: replace `path` with `contract_file`
    pub fn replace_empty_code(&mut self, contract_name: &str, contract_path: &str) {
        for action in self.actions.iter_mut() {
            let SpecsAction::Instantiate { code, .. } = action else {
                continue;
            };

            match code {
                Code::Bytes(bytes) if bytes.is_empty() => {
                    let contract_source = match std::fs::read_to_string(contract_path) {
                        Err(err) => panic!("unable to read {contract_path}: {err}"),
                        Ok(solidity) => solidity,
                    };
                    *bytes = compile_blob(contract_name, &contract_source)
                }
                Code::Solidity { path, .. } if path.is_none() => *path = Some(contract_path.into()),
                _ => continue,
            }
        }
    }

    /// Run a contract test
    /// The test takes a [`Specs`] and executes the actions in order
    pub fn run(self) -> Vec<CallResult> {
        if self.differential {
            self.run_on_evm()
        } else {
            self
        }
        .run_on_pallet()
    }

    fn run_on_evm(self) -> Self {
        let mut specs = Self {
            actions: vec![],
            ..self
        };

        for action in self.actions {
            specs.actions.push(action.clone());

            use specs::SpecsAction::*;
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
                    let Code::Solidity {
                        path: Some(path),
                        solc_optimizer,
                        pipeline,
                        contract,
                    } = code
                    else {
                        panic!("the differential runner requires Code::Solidity source");
                    };
                    assert!(storage_deposit_limit.is_none(), "storage deposit limit is not supported in differential mode");
                    assert!(salt.is_empty(), "salt is not supported in differential mode");
                    let deploy_code = match std::fs::read_to_string(&path) {
                        Ok(solidity_source) => compile_evm_deploy_code(&contract, &solidity_source),
                        Err(err) => panic!(
                            "failed to read solidity source\n .  path: '{}'\n .   error: {:?}",
                            path.display(),
                            err
                        ),
                    };
                }
                Call {
                    origin,
                    dest,
                    value,
                    gas_limit,
                    storage_deposit_limit,
                    data,
                } => {
                    //let TestAccountId::Instantiated(n) = dest else {
                    //    panic!("the differential runner requires TestAccountId::Instantiated(n) as dest");
                    //};
                }
                _ => panic!("only instantiate and call action allowed in differential mode, got: {action:?}"),
            }
        }

        specs
    }

    fn run_on_pallet(self) -> Vec<CallResult> {
        let mut results = vec![];

        ExtBuilder::default()
            .balance_genesis_config(self.balances.clone())
            .build()
            .execute_with(|| {
                use specs::SpecsAction::*;

                let actions = self.actions();

                for action in self.actions() {
                    match action {
                        Instantiate {
                            origin,
                            value,
                            gas_limit,
                            storage_deposit_limit,
                            code,
                            data,
                            salt,
                        } => results.push(CallResult::Instantiate(Contracts::bare_instantiate(
                            RuntimeOrigin::signed(origin.to_account_id(&results)),
                            value,
                            gas_limit.unwrap_or(GAS_LIMIT),
                            storage_deposit_limit.unwrap_or(DEPOSIT_LIMIT),
                            code.into(),
                            data,
                            salt,
                            DebugInfo::Skip,
                            CollectEvents::Skip,
                        ))),
                        Call {
                            origin,
                            dest,
                            value,
                            gas_limit,
                            storage_deposit_limit,
                            data,
                        } => results.push(CallResult::Exec(Contracts::bare_call(
                            RuntimeOrigin::signed(origin.to_account_id(&results)),
                            dest.to_account_id(&results),
                            value,
                            gas_limit.unwrap_or(GAS_LIMIT),
                            storage_deposit_limit.unwrap_or(DEPOSIT_LIMIT),
                            data,
                            DebugInfo::Skip,
                            CollectEvents::Skip,
                        ))),
                        VerifyCall(expectation) => {
                            expectation.verify(results.last().expect("No call to verify"));
                        }
                        VerifyBalance { origin, expected } => {
                            let balance = Balances::free_balance(origin.to_account_id(&results));
                            assert_eq!(balance, expected);
                        }
                        VerifyStorage {
                            contract,
                            key,
                            expected,
                        } => {
                            let Ok(storage) = Contracts::get_storage(
                                contract.to_account_id(&results),
                                key.clone(),
                            ) else {
                                panic!("Error reading storage");
                            };
                            let Some(value) = storage else {
                                panic!("No value for storage key 0x{}", hex::encode(key));
                            };
                            assert_eq!(value, expected);
                        }
                    }
                }
            });

        match &results[0] {
            CallResult::Instantiate(res) => res.result.as_ref().unwrap().account_id.clone(),
            _ => todo!(),
        };

        results
    }
}

pub trait SpecsRunner {
    fn run_action(&mut self, spec: &mut Specs) -> Vec<CallResult>;
}
