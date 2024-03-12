//!
//! The compiler warning.
//!

use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

///
/// The compiler warning.
///
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Warning {
    /// The warning for eponymous feature.
    EcRecover,
    /// The warning for eponymous feature.
    SendTransfer,
    /// The warning for eponymous feature.
    ExtCodeSize,
    /// The warning for eponymous feature.
    TxOrigin,
    /// The warning for eponymous feature.
    BlockTimestamp,
    /// The warning for eponymous feature.
    BlockNumber,
    /// The warning for eponymous feature.
    BlockHash,
}

impl Warning {
    ///
    /// Converts string arguments into an array of warnings.
    ///
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
