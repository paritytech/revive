use core::str;
use std::{
    collections::BTreeMap,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    str::FromStr,
};

use alloy_genesis::{Genesis, GenesisAccount};
use alloy_primitives::{hex::ToHexExt, Address, Bytes, B256, U256};
use alloy_serde::storage::deserialize_storage_map;
use serde::{Deserialize, Serialize};
use serde_json::{Deserializer, Value};
use tempfile::NamedTempFile;

const GENESIS_JSON: &str = include_str!("../genesis.json");
const EXECUTABLE_NAME: &str = "evm";
const EXECUTABLE_ARGS: [&str; 8] = [
    "--log.format=json",
    "run",
    "--dump",
    "--nomemory=false",
    "--noreturndata=false",
    "--json",
    "--codefile",
    "-",
];

/// The geth EVM state dump structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateDump {
    pub root: Bytes,
    pub accounts: BTreeMap<Address, Account>,
}

impl From<StateDump> for Genesis {
    fn from(value: StateDump) -> Self {
        let mut genesis: Genesis = serde_json::from_str(GENESIS_JSON).unwrap();
        genesis.alloc = value
            .accounts
            .iter()
            .map(|(address, account)| (*address, account.clone().into()))
            .collect();
        genesis
    }
}

/// The geth EVM state dump account structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub balance: U256,
    pub nonce: u64,
    pub code: Option<Bytes>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_storage_map"
    )]
    pub storage: Option<BTreeMap<B256, B256>>,
    pub key: Option<B256>,
}

impl From<Account> for GenesisAccount {
    fn from(value: Account) -> Self {
        GenesisAccount {
            balance: value.balance,
            nonce: Some(value.nonce),
            code: value.code,
            storage: value.storage,
            private_key: value.key,
        }
    }
}

/// Contains the output from geth `emv` invocations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvmOutput {
    pub output: Bytes,
    #[serde(rename = "gasUsed")]
    pub gas_used: U256,
    pub error: Option<String>,
}

impl EvmOutput {
    /// Return if there was no error found.
    ///
    /// Panics if the gas used is zero as this indicates nothing was run, i.e.
    /// there was no receiving account but still no error is reported.
    pub fn run_success(&self) -> bool {
        assert_ne!(self.gas_used, U256::ZERO, "nothing was executed");
        self.error.is_none()
    }
}

/// Contains the full log from geth `emv` invocations
#[derive(Clone, Debug)]
pub struct EvmLog {
    pub account_deployed: Option<Address>,
    pub output: EvmOutput,
    pub state_dump: StateDump,
}

impl From<&str> for EvmLog {
    fn from(value: &str) -> Self {
        let mut output = None;
        let mut state_dump = None;
        for value in Deserializer::from_str(value).into_iter::<Value>() {
            let Ok(value) = value else { continue };
            if let Ok(value @ EvmOutput { .. }) = serde_json::from_value(value.clone()) {
                output = Some(value);
                continue;
            }
            if let Ok(value @ StateDump { .. }) = serde_json::from_value(value) {
                state_dump = Some(value);
            }
        }

        Self {
            account_deployed: None,
            output: output.expect("the EVM log should contain the output"),
            state_dump: state_dump.expect("the EVM log should contain the state dump"),
        }
    }
}

/// Builder for running contracts in geth `evm`
pub struct Evm {
    genesis_json: String,
    code: Option<Vec<u8>>,
    input: Option<Bytes>,
    receiver: Option<String>,
    sender: String,
    value: Option<u128>,
    gas: Option<u64>,
    create: bool,
}

impl Default for Evm {
    fn default() -> Self {
        Self {
            genesis_json: GENESIS_JSON.to_string(),
            code: None,
            input: None,
            receiver: None,
            sender: Address::default().encode_hex(),
            value: None,
            gas: None,
            create: false,
        }
    }
}

impl Evm {
    /// Create a new EVM with the given `genesis`
    pub fn from_genesis(genesis: Genesis) -> Self {
        Self::default().genesis_json(genesis)
    }

    /// Run the code found in `code_file`
    pub fn code_file(self, path: PathBuf) -> Self {
        Self {
            code: std::fs::read_to_string(&path)
                .unwrap_or_else(|err| {
                    panic!("can not read EVM byte code from file {path:?}: {err}")
                })
                .into_bytes()
                .into(),
            ..self
        }
    }

    /// Run the `code`
    pub fn code_blob(self, blob: Vec<u8>) -> Self {
        Self {
            code: Some(blob),
            ..self
        }
    }

    /// Set the calldata
    pub fn input(self, bytes: Bytes) -> Self {
        Self {
            input: Some(bytes),
            ..self
        }
    }

    /// Set the create flag
    pub fn deploy(self, enable: bool) -> Self {
        Self {
            create: enable,
            ..self
        }
    }

    /// Set the transferred value
    pub fn value(self, value: u128) -> Self {
        Self {
            value: Some(value),
            ..self
        }
    }

    /// Set the gas limit
    pub fn gas(self, limit: u64) -> Self {
        Self {
            gas: Some(limit),
            ..self
        }
    }

    /// Provide the prestate genesis configuration
    pub fn genesis_json(self, genesis: Genesis) -> Self {
        let genesis_json = serde_json::to_string(&genesis).expect("state dump should be valid");
        // TODO: Investigate
        let genesis_json = genesis_json.replace("\"0x0\"", "0");

        Self {
            genesis_json,
            ..self
        }
    }

    /// Set the callee address
    pub fn receiver(self, address: Address) -> Self {
        Self {
            receiver: Some(address.encode_hex()),
            ..self
        }
    }

    /// Set the caller address
    pub fn sender(self, address: Address) -> Self {
        Self {
            sender: address.encode_hex(),
            ..self
        }
    }

    /// Calculate the address of the contract account this deploy call would create
    pub fn expect_account_created(&self) -> Address {
        assert!(self.create, "expected a deploy call");
        let sender = Address::from_str(&self.sender).expect("sender address should be valid");
        let genesis: Genesis = serde_json::from_str(&self.genesis_json).unwrap();
        let nonce = genesis
            .alloc
            .get(&sender)
            .map(|account| account.nonce.unwrap_or(0))
            .unwrap_or(0);
        sender.create(nonce)
    }

    /// Run the call in a geth `evm` subprocess
    pub fn run(self) -> EvmLog {
        // Write genesis.json to disk
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(self.genesis_json.as_bytes()).unwrap();
        let path = temp_file.into_temp_path();
        let genesis_json_path = &path.display().to_string();

        // Static args
        let mut command = Command::new(PathBuf::from(EXECUTABLE_NAME));
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.args(EXECUTABLE_ARGS);
        command.args(["--prestate", genesis_json_path]);

        // Dynamic args
        if let Some(input) = &self.input {
            command.args(["--input", hex::encode(input).as_str()]);
        }
        let account_deployed = if self.create {
            command.arg("--create");
            self.expect_account_created().into()
        } else {
            None
        };
        command.args(["--sender", &self.sender]);
        match (&self.code, &self.receiver) {
            (Some(_), None) => {}
            (None, Some(address)) => {
                command.args(["--receiver", address]);
            }
            (Some(_), Some(_)) => panic!("code and receiver specified"),
            _ => panic!("no code file or receiver specified"),
        }
        if let Some(gas) = self.gas {
            command.args(["--gas", &format!("{gas}")]);
        }
        if let Some(value) = self.value {
            command.args(["--value", &format!("{value}")]);
        }

        // Run the evm subprocess and assert success return value
        let process = command.spawn().unwrap_or_else(|error| {
            panic!("{EXECUTABLE_NAME} subprocess spawning error: {error:?}")
        });
        let buf = vec![];
        process
            .stdin
            .as_ref()
            .unwrap_or_else(|| panic!("{EXECUTABLE_NAME} stdin getting error"))
            .write_all(self.code.as_ref().unwrap_or(&buf))
            .unwrap_or_else(|err| panic!("{EXECUTABLE_NAME} stdin writing error: {err:?}"));

        let output = process
            .wait_with_output()
            .unwrap_or_else(|err| panic!("{EXECUTABLE_NAME} subprocess output error: {err}"));
        assert!(
            output.status.success(),
            "{EXECUTABLE_NAME} command failed: {output:?}",
        );

        // Set the deployed account
        let mut log: EvmLog = str::from_utf8(output.stdout.as_slice())
            .unwrap_or_else(|err| panic!("{EXECUTABLE_NAME} output failed to parse: {err}"))
            .into();
        log.account_deployed = account_deployed;
        log
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use alloy_genesis::Genesis;
    use alloy_primitives::{Bytes, B256, U256};

    use crate::{Evm, EvmLog, EvmOutput, StateDump};

    const OUTPUT_OK: &str = r#"{"output":"0000000000000000000000000000000000000000000000000000000000000000","gasUsed":"0x11d"}"#;
    const OUTPUT_REVERTED: &str = r#"{"output":"","gasUsed":"0x2d","error":"execution reverted"}"#;
    const STATE_DUMP: &str = r#"
{
    "root": "eb5d51177cb9049b848ea92f87f9a3f00abfb683d0866c2eddecc5692ad27f86",
    "accounts": {
        "0x1f2a98889594024BFfdA3311CbE69728d392C06D": {
            "balance": "0",
            "nonce": 1,
            "root": "0x63cfcda8d81a8b1840b1b9722c37f929a4037e53ad1ce6abdef31c0c8bac1f61",
            "codeHash": "0xa6e0062c5ba829446695f179b97702a75f7d354e33445d2e928ed00e1a39e88f",
            "code": "0x608060405260043610610028575f3560e01c80633fa4f2451461002c578063b144adfb1461004a575b5f80fd5b610034610086565b60405161004191906100c5565b60405180910390f35b348015610055575f80fd5b50610070600480360381019061006b919061013c565b61008d565b60405161007d91906100c5565b60405180910390f35b5f34905090565b5f8173ffffffffffffffffffffffffffffffffffffffff16319050919050565b5f819050919050565b6100bf816100ad565b82525050565b5f6020820190506100d85f8301846100b6565b92915050565b5f80fd5b5f73ffffffffffffffffffffffffffffffffffffffff82169050919050565b5f61010b826100e2565b9050919050565b61011b81610101565b8114610125575f80fd5b50565b5f8135905061013681610112565b92915050565b5f60208284031215610151576101506100de565b5b5f61015e84828501610128565b9150509291505056fea2646970667358221220a2109c2f05a629fff4640e9f0cf12a698bbea9b0858a4029901e88bf5d1c926964736f6c63430008190033",
            "storage": {
                "0x0000000000000000000000000000000000000000000000000000000000000000": "02"
            },
            "address": "0x1f2a98889594024bffda3311cbe69728d392c06d",
            "key": "0xcbeeb4463624bc2f332dcfe2b479eddb1c380ec862ee63d9f31b31b854fb7c61"
        }
    }
}"#;
    const EVM_BIN_FIXTURE: &str = "6080604052348015600e575f80fd5b506040516101403803806101408339818101604052810190602e9190607f565b805f806101000a81548160ff0219169083151502179055505060a5565b5f80fd5b5f8115159050919050565b606181604f565b8114606a575f80fd5b50565b5f81519050607981605a565b92915050565b5f602082840312156091576090604b565b5b5f609c84828501606d565b91505092915050565b608f806100b15f395ff3fe6080604052348015600e575f80fd5b50600436106026575f3560e01c8063cde4efa914602a575b5f80fd5b60306032565b005b5f8054906101000a900460ff16155f806101000a81548160ff02191690831515021790555056fea264697066735822122046c92dd2fd612b1ed93d184dad4c49f61c44690722c4a6c7c746ebeb0aadeb4a64736f6c63430008190033";
    const EVM_BIN_RUNTIME_FIXTURE: &str = "6080604052348015600e575f80fd5b50600436106026575f3560e01c8063cde4efa914602a575b5f80fd5b60306032565b005b5f8054906101000a900460ff16155f806101000a81548160ff02191690831515021790555056fea264697066735822122046c92dd2fd612b1ed93d184dad4c49f61c44690722c4a6c7c746ebeb0aadeb4a64736f6c63430008190033";
    const EVM_BIN_FIXTURE_INPUT: &str =
        "0000000000000000000000000000000000000000000000000000000000000001";
    const EVM_BIN_RUNTIME_FIXTURE_INPUT: &str = "cde4efa9";

    #[test]
    fn parse_evm_output_ok() {
        serde_json::from_str::<EvmOutput>(OUTPUT_OK).unwrap();
    }

    #[test]
    fn parse_evm_output_revert() {
        serde_json::from_str::<EvmOutput>(OUTPUT_REVERTED).unwrap();
    }

    #[test]
    fn parse_state_dump() {
        serde_json::from_str::<StateDump>(STATE_DUMP).unwrap();
    }

    #[test]
    fn evm_log_from_str() {
        let log = format!("{OUTPUT_OK}\n{STATE_DUMP}");
        let _ = EvmLog::from(log.as_str());
    }

    #[test]
    fn generate_genesis() {
        let log = format!("{OUTPUT_OK}\n{STATE_DUMP}");
        let log = EvmLog::from(log.as_str());
        let mut genesis: Genesis = log.state_dump.into();
        let storage = genesis
            .alloc
            .pop_first()
            .expect("should have one account in genesis")
            .1
            .storage
            .expect("genesis account should have storage");
        let storage_value = storage
            .get(&B256::ZERO)
            .expect("genesis account should have key 0 occupied");
        assert_eq!(*storage_value, B256::from(U256::from(2)));
    }

    #[test]
    fn flipper() {
        let log_runtime = Evm::default()
            .code_blob(EVM_BIN_RUNTIME_FIXTURE.as_bytes().to_vec())
            .input(Bytes::from_str(EVM_BIN_RUNTIME_FIXTURE_INPUT).unwrap())
            .run();
        assert!(log_runtime.output.run_success());
    }

    #[test]
    fn prestate() {
        let log_deploy = Evm::default()
            .code_blob(EVM_BIN_FIXTURE.as_bytes().to_vec())
            .input(Bytes::from_str(EVM_BIN_FIXTURE_INPUT).unwrap())
            .deploy(true)
            .run();
        assert!(log_deploy.output.run_success());

        let address = log_deploy.account_deployed.unwrap();
        let genesis: Genesis = log_deploy.state_dump.into();
        let log_runtime = Evm::default()
            .genesis_json(genesis)
            .receiver(address)
            .input(Bytes::from_str(EVM_BIN_RUNTIME_FIXTURE_INPUT).unwrap())
            .run();
        assert!(log_runtime.output.run_success(), "{:?}", log_runtime.output);
    }
}
