//! `resolc` custom compiler warnings.
//!
//! The revive compiler adds warnings only applicable when compilng
//! to the revive stack on Polkadot to the output.

use std::collections::BTreeMap;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

use crate::standard_json::output::error::source_location::SourceLocation;
use crate::SolcStandardJsonInputSource;
use crate::SolcStandardJsonOutputError;

// The `resolc` custom compiler warning.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Warning {
    /// The `<address payable>`'s `send` and `transfer` methods usage warning.
    SendAndTransfer,
    /// The `origin` instruction usage warning.
    TxOrigin,
}

impl Warning {
    /// Converts string arguments into an array of warnings.
    pub fn try_from_strings(strings: &[String]) -> Result<Vec<Self>, anyhow::Error> {
        strings
            .iter()
            .map(|string| Self::from_str(string))
            .collect()
    }

    /// The displayed warning messages.
    pub fn as_message(&self) -> &'static str {
        match self {
            Self::SendAndTransfer => {
                r#"
Warning: It looks like you are using '<address payable>.send/transfer(<X>)'.
Using '<address payable>.send/transfer(<X>)' is deprecated and strongly discouraged!
The resolc compiler uses a heuristic to detect '<address payable>.send/transfer(<X>)' calls,
which disables call re-entrancy and supplies all remaining gas instead of the 2300 gas stipend.
However, detection is not guaranteed. You are advised to carefully test this, employ
re-entrancy guards or use the withdrawal pattern instead!
Learn more on https://docs.soliditylang.org/en/latest/security-considerations.html#reentrancy
and https://docs.soliditylang.org/en/latest/common-patterns.html#withdrawal-from-contracts
"#
            }
            Self::TxOrigin => {
                r#"
Warning: You are checking for 'tx.origin' in your code, which might lead to unexpected behavior.
Polkadot comes with native account abstraction support, and therefore the initiator of a
transaction might be different from the contract calling your code. It is highly recommended NOT
to rely on tx.origin, but use msg.sender instead.
"#
            }
        }
    }

    pub fn as_error(
        &self,
        node: Option<&str>,
        id_paths: &BTreeMap<usize, &String>,
        sources: &BTreeMap<String, SolcStandardJsonInputSource>,
    ) -> SolcStandardJsonOutputError {
        SolcStandardJsonOutputError::new_warning(
            self.as_message(),
            node.and_then(|node| SourceLocation::try_from_ast(node, id_paths)),
            Some(sources),
        )
    }
}

impl FromStr for Warning {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "sendandtransfer" => Ok(Self::SendAndTransfer),
            "txorigin" => Ok(Self::TxOrigin),
            _ => Err(anyhow::anyhow!("Invalid warning: {}", string)),
        }
    }
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::SendAndTransfer => write!(f, "sendandtransfer"),
            Self::TxOrigin => write!(f, "txorigin"),
        }
    }
}
