use std::time::Instant;

use revive_differential::{Evm, EvmLog};
use serde::{Deserialize, Serialize};

use crate::*;
use alloy_primitives::Address;
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

impl SpecsAction {
    /// Derive verification actions from the EVM output log
    pub fn derive_verification(
        log: &EvmLog,
        address_evm: Address,
        account_pvm: TestAccountId,
    ) -> Vec<Self> {
        let account = log
            .state_dump
            .accounts
            .get(&address_evm)
            .unwrap_or_else(|| panic!("account {address_evm} not in state dump"));

        let mut actions = vec![
            Self::VerifyCall(VerifyCallExpectation {
                gas_consumed: None,
                success: log.output.run_success(),
                output: log.output.output.clone().into(),
            }),
            Self::VerifyBalance {
                origin: account_pvm.clone(),
                expected: account
                    .balance
                    .try_into()
                    .expect("balance should fit into u128"),
            },
        ];

        let Some(storage) = &account.storage else {
            return actions;
        };

        for (key, expected) in storage {
            let mut key = key.to_vec();
            key.reverse();
            let mut expected = expected.to_vec();
            expected.reverse();
            actions.push(Self::VerifyStorage {
                contract: account_pvm.clone(),
                key,
                expected,
            });
        }

        actions
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum TestAccountId {
    /// The ALICE account
    #[default]
    Alice,
    /// The BOB account
    Bob,
    /// The CHARLIE account
    Charlie,
    /// AccountID that was created during the nth call in this run.
    Instantiated(usize),
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
                .get(*n)
                .expect("should provide valid index into call results")
            {
                CallResult::Exec { .. } => panic!("call #{n} should be an instantiation"),
                CallResult::Instantiate { result, .. } => result
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
    /// missing and not in differential mode
    pub fn actions(&self) -> Vec<SpecsAction> {
        self.actions
            .iter()
            .enumerate()
            .flat_map(|(index, item)| {
                let next_item = self.actions.get(index + 1);
                if matches!(
                    item,
                    SpecsAction::Instantiate { .. } | SpecsAction::Call { .. }
                ) && matches!(
                    next_item,
                    Some(SpecsAction::Instantiate { .. }) | Some(SpecsAction::Call { .. })
                ) && !self.differential
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
        let mut derived_specs = Self {
            actions: vec![],
            ..self
        };

        let mut evm = Evm::default();
        let mut deployed_accounts = vec![];

        for action in self.actions {
            derived_specs.actions.push(action.clone());

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
                    assert_ne!(solc_optimizer, Some(false), "solc_optimizer must be enabled in differntial mode");
                    assert_ne!(pipeline, Some(revive_solidity::SolcPipeline::EVMLA), "yul pipeline must be enabled in differntial mode");
                    assert!(storage_deposit_limit.is_none(), "storage deposit limit is not supported in differential mode");
                    assert!(salt.is_empty(), "salt is not supported in differential mode");
                    assert_eq!(origin, TestAccountId::default(), "configuring the origin is not supported in differential mode");
                    let deploy_code = match std::fs::read_to_string(&path) {
                        Ok(solidity_source) => compile_evm_deploy_code(&contract, &solidity_source),
                        Err(err) => panic!(
                            "failed to read solidity source\n .  path: '{}'\n .   error: {:?}",
                            path.display(),
                            err
                        ),
                    };
                    let deploy_code = hex::encode(deploy_code);
                    let mut vm = evm.code_blob(deploy_code.as_bytes().to_vec()).sender(Address::default()).deploy(true);
                    if !data.is_empty() {
                        vm = vm.input(data.into());
                    }
                    if value > 0 {
                        vm = vm.value(value);
                    }
                    if let Some(gas) = gas_limit {
                        vm = vm.gas(gas.ref_time());
                    }
                    let mut log = vm.run();
                    log.output.output = Default::default(); // PVM will not have constructor output
                    let deployed_account = log.account_deployed.expect("no account was created");
                    let account_pvm = TestAccountId::Instantiated(deployed_accounts.len());
                    deployed_accounts.push(deployed_account);
                    derived_specs.actions.append(&mut SpecsAction::derive_verification(&log, deployed_account, account_pvm));
                    evm = Evm::from_genesis(log.state_dump.into());
                }
                Call {
                    origin,
                    dest,
                    value,
                    gas_limit,
                    storage_deposit_limit,
                    data,
                } => {
                    assert_eq!(origin, TestAccountId::default(), "configuring the origin is not supported in differential mode");
                    assert!(storage_deposit_limit.is_none(), "storage deposit limit is not supported in differential mode");
                    let TestAccountId::Instantiated(n) = dest else {
                        panic!("the differential runner requires TestAccountId::Instantiated(n) as dest");
                    };
                    let address = deployed_accounts.get(n).unwrap_or_else(|| panic!("no account at index {n} "));
                    let mut vm = evm.receiver(*address).sender(Address::default());
                    if !data.is_empty() {
                        vm = vm.input(data.into());
                    }
                    if value > 0 {
                        vm = vm.value(value);
                    }
                    if let Some(gas) = gas_limit {
                        vm = vm.gas(gas.ref_time());
                    }

                    let log = vm.run();
                    derived_specs.actions.append(&mut SpecsAction::derive_verification(&log, *address, dest));
                    evm = Evm::from_genesis(log.state_dump.into());
                }
                _ => panic!("only instantiate and call action allowed in differential mode, got: {action:?}"),
            }
        }

        derived_specs
    }

    fn run_on_pallet(self) -> Vec<CallResult> {
        let mut results = vec![];

        ExtBuilder::default()
            .balance_genesis_config(self.balances.clone())
            .build()
            .execute_with(|| {
                use specs::SpecsAction::*;

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
                        } => {
                            let origin = RuntimeOrigin::signed(origin.to_account_id(&results));
                            let time_start = Instant::now();
                            let result = Contracts::bare_instantiate(
                                origin,
                                value,
                                gas_limit.unwrap_or(GAS_LIMIT),
                                storage_deposit_limit.unwrap_or(DEPOSIT_LIMIT),
                                code.into(),
                                data,
                                salt,
                                DebugInfo::Skip,
                                CollectEvents::Skip,
                            );
                            results.push(CallResult::Instantiate {
                                result,
                                wall_time: time_start.elapsed(),
                            })
                        }
                        Call {
                            origin,
                            dest,
                            value,
                            gas_limit,
                            storage_deposit_limit,
                            data,
                        } => {
                            let origin = RuntimeOrigin::signed(origin.to_account_id(&results));
                            let dest = dest.to_account_id(&results);
                            let time_start = Instant::now();
                            let result = Contracts::bare_call(
                                origin,
                                dest,
                                value,
                                gas_limit.unwrap_or(GAS_LIMIT),
                                storage_deposit_limit.unwrap_or(DEPOSIT_LIMIT),
                                data,
                                DebugInfo::Skip,
                                CollectEvents::Skip,
                            );
                            results.push(CallResult::Exec {
                                result,
                                wall_time: time_start.elapsed(),
                            });
                        }
                        VerifyCall(expectation) => {
                            expectation.verify(results.last().expect("No call to verify"));
                        }
                        VerifyBalance { origin, expected } => {
                            let balance = Balances::usable_balance(origin.to_account_id(&results));
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
                            assert_eq!(value, expected, "at key {}", hex::encode(&key));
                        }
                    }
                }
            });

        results
    }
}

pub trait SpecsRunner {
    fn run_action(&mut self, spec: &mut Specs) -> Vec<CallResult>;
}
