//! `resolc` custom compiler warnings.
//!
//! The revive compiler adds warnings only applicable when compilng
//! to the revive stack on Polkadot to the output.

use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

// The `resolc` custom compiler warning.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Warning {
    EcRecover,
    SendTransfer,
    ExtCodeSize,
    TxOrigin,
    BlockTimestamp,
    BlockNumber,
    BlockHash,
}

impl Warning {
    /// Converts string arguments into an array of warnings.
    pub fn try_from_strings(strings: &[String]) -> Result<Vec<Self>, anyhow::Error> {
        strings
            .iter()
            .map(|string| Self::from_str(string))
            .collect()
    }
}

impl FromStr for Warning {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "ecrecover" => Ok(Self::EcRecover),
            "sendtransfer" => Ok(Self::SendTransfer),
            "extcodesize" => Ok(Self::ExtCodeSize),
            "txorigin" => Ok(Self::TxOrigin),
            "blocktimestamp" => Ok(Self::BlockTimestamp),
            "blocknumber" => Ok(Self::BlockNumber),
            "blockhash" => Ok(Self::BlockHash),
            _ => Err(anyhow::anyhow!("Invalid warning: {}", string)),
        }
    }
}
