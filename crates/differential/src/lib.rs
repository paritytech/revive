use std::collections::BTreeMap;

use alloy_genesis::{Genesis, GenesisAccount};
use alloy_primitives::{Address, Bytes, B256, U256};
use alloy_serde::storage::deserialize_storage_map;
use serde::{Deserialize, Serialize};
use serde_json::{Deserializer, Value};

pub const GENESIS_JSON: &str = include_str!("../genesis.json");

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

        let stream = Deserializer::from_str(value).into_iter::<Value>();
        for value in stream {
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

#[cfg(test)]
mod tests {
    use alloy_genesis::Genesis;
    use alloy_primitives::{B256, U256};

    use crate::{EvmLog, EvmOutput, StateDump};

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
}
