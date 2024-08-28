use core::str;
use std::{
    collections::BTreeMap,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use alloy_genesis::{Genesis, GenesisAccount};
use alloy_primitives::{Address, Bytes, B256, U256};
use alloy_serde::storage::deserialize_storage_map;
use serde::{Deserialize, Serialize};
use serde_json::{Deserializer, Value};

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateDump {
    pub root: Bytes,
    pub accounts: BTreeMap<Address, Account>,
}

impl Into<Genesis> for StateDump {
    fn into(self) -> Genesis {
        let mut genesis: Genesis = serde_json::from_str(GENESIS_JSON).unwrap();
        genesis.alloc = self
            .accounts
            .iter()
            .map(|(address, account)| (*address, account.clone().into()))
            .collect();
        genesis
    }
}

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

impl Into<GenesisAccount> for Account {
    fn into(self) -> GenesisAccount {
        GenesisAccount {
            balance: self.balance,
            nonce: Some(self.nonce),
            code: self.code,
            storage: self.storage,
            private_key: self.key,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvmOutput {
    output: Bytes,
    #[serde(rename = "gasUsed")]
    gas_used: U256,
    error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct EvmLog {
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
            output: output.expect("the EVM log should contain the output"),
            state_dump: state_dump.expect("the EVM log should contain the state dump"),
        }
    }
}

#[derive(Debug, Default)]
struct Evm {
    genesis_path: Option<PathBuf>,
    code: Option<Vec<u8>>,
    input: Option<Bytes>,
    create: bool,
}

impl Evm {
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

    pub fn genesis_json(self, path: PathBuf) -> Self {
        Self {
            genesis_path: Some(path),
            ..self
        }
    }

    pub fn run(&self) -> EvmLog {
        let Some(code) = &self.code else {
            panic!("no code or code file specified")
        };

        let genesis_json_path = "/tmp/genesis.json";
        std::fs::write(genesis_json_path, GENESIS_JSON)
            .unwrap_or_else(|err| panic!("failed to write genesis.json: {err}"));
        let mut command = Command::new(PathBuf::from(EXECUTABLE_NAME));
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.args(EXECUTABLE_ARGS);
        command.args(["--prestate", genesis_json_path]);
        if let Some(input) = &self.input {
            command.args(["--input", hex::encode(input).as_str()]);
        }
        if self.create {
            command.arg("--create");
        }

        let process = command.spawn().unwrap_or_else(|error| {
            panic!("{EXECUTABLE_NAME} subprocess spawning error: {error:?}")
        });
        process
            .stdin
            .as_ref()
            .unwrap_or_else(|| panic!("{EXECUTABLE_NAME} stdin getting error"))
            .write_all(code)
            .unwrap_or_else(|err| panic!("{EXECUTABLE_NAME} stdin writing error: {err:?}"));

        let output = process
            .wait_with_output()
            .unwrap_or_else(|err| panic!("{EXECUTABLE_NAME} subprocess output error: {err}"));

        assert!(
            output.status.success(),
            "{EXECUTABLE_NAME} command failed: {}",
            output.status
        );

        str::from_utf8(output.stdout.as_slice())
            .unwrap_or_else(|err| panic!("{EXECUTABLE_NAME} output failed to parse: {err}"))
            .into()
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
        let log_deploy = Evm::default()
            .code_blob(EVM_BIN_FIXTURE.as_bytes().to_vec())
            .input(Bytes::from_str(EVM_BIN_FIXTURE_INPUT).unwrap())
            .deploy(true)
            .run();
        assert!(log_deploy.output.error.is_none());

        let log_runtime = Evm::default()
            .code_blob(EVM_BIN_RUNTIME_FIXTURE.as_bytes().to_vec())
            .input(Bytes::from_str(EVM_BIN_RUNTIME_FIXTURE_INPUT).unwrap())
            .run();
        assert!(log_runtime.output.error.is_none());
    }
}
