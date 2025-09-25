//! The `solc --standard-json` output error.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

use crate::SolcStandardJsonInputSource;

use self::mapped_location::MappedLocation;
use self::source_location::SourceLocation;

pub mod error_handler;
pub mod mapped_location;
pub mod source_location;

/// The `solc --standard-json` output error.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    /// The component type.
    pub component: String,
    /// The error code.
    pub error_code: Option<String>,
    /// The formatted error message.
    pub formatted_message: String,
    /// The non-formatted error message.
    pub message: String,
    /// The error severity.
    pub severity: String,
    /// The error location data.
    pub source_location: Option<SourceLocation>,
    /// The error type.
    pub r#type: String,
}

impl Error {
    /// The list of ignored `solc` warnings that are strictly EVM-related.
    pub const IGNORED_WARNING_CODES: [&'static str; 5] = ["1699", "3860", "5159", "5574", "6417"];

    /// A shortcut constructor.
    pub fn new<S>(
        r#type: &str,
        message: S,
        source_location: Option<SourceLocation>,
        sources: Option<&BTreeMap<String, SolcStandardJsonInputSource>>,
    ) -> Self
    where
        S: std::fmt::Display,
    {
        let message = message.to_string();

        let message_trimmed = message.trim();
        let mut formatted_message = if message_trimmed.starts_with(r#type) {
            message_trimmed.to_owned()
        } else {
            format!("{type}: {message_trimmed}")
        };
        formatted_message.push('\n');
        if let Some(ref source_location) = source_location {
            let source_code = sources.and_then(|sources| {
                sources
                    .get(source_location.file.as_str())
                    .and_then(|source| source.content())
            });
            let mapped_location =
                MappedLocation::try_from_source_location(source_location, source_code);
            formatted_message.push_str(mapped_location.to_string().as_str());
            formatted_message.push('\n');
        }

        Self {
            component: "general".to_owned(),
            error_code: None,
            formatted_message,
            message,
            severity: r#type.to_lowercase(),
            source_location,
            r#type: r#type.to_owned(),
        }
    }

    /// A shortcut constructor.
    pub fn new_error<S>(
        message: S,
        source_location: Option<SourceLocation>,
        sources: Option<&BTreeMap<String, SolcStandardJsonInputSource>>,
    ) -> Self
    where
        S: std::fmt::Display,
    {
        Self::new("Error", message, source_location, sources)
    }

    /// A shortcut constructor.
    pub fn new_warning<S>(
        message: S,
        source_location: Option<SourceLocation>,
        sources: Option<&BTreeMap<String, SolcStandardJsonInputSource>>,
    ) -> Self
    where
        S: std::fmt::Display,
    {
        Self::new("Warning", message, source_location, sources)
    }

    /// Returns the `<address payable>`'s `send` and `transfer` methods usage error.
    pub fn warning_send_and_transfer(
        node: Option<&str>,
        id_paths: &BTreeMap<usize, &String>,
        sources: &BTreeMap<String, SolcStandardJsonInputSource>,
    ) -> Self {
        let message = r#"
Warning: It looks like you are using '<address payable>.send/transfer(<X>)'.
Using '<address payable>.send/transfer(<X>)' is deprecated and strongly discouraged!
The resolc compiler uses a heuristic to detect '<address payable>.send/transfer(<X>)' calls,
which disables call re-entrancy and supplies all remaining gas instead of the 2300 gas stipend.
However, detection is not guaranteed. You are advised to carefully test this, employ
re-entrancy guards or use the withdrawal pattern instead!
Learn more on https://docs.soliditylang.org/en/latest/security-considerations.html#reentrancy
and https://docs.soliditylang.org/en/latest/common-patterns.html#withdrawal-from-contracts
"#
        .to_owned();

        Self::new_warning(
            message,
            node.and_then(|node| SourceLocation::try_from_ast(node, id_paths)),
            Some(sources),
        )
    }

    /// Returns the `origin` instruction usage warning.
    pub fn warning_tx_origin(
        node: Option<&str>,
        id_paths: &BTreeMap<usize, &String>,
        sources: &BTreeMap<String, SolcStandardJsonInputSource>,
    ) -> Self {
        let message = r#"
Warning: You are checking for 'tx.origin' in your code, which might lead to unexpected behavior.
Polkadot comes with native account abstraction support, and therefore the initiator of a
transaction might be different from the contract calling your code. It is highly recommended NOT
to rely on tx.origin, but use msg.sender instead.
"#
        .to_owned();

        Self::new_warning(
            message,
            node.and_then(|node| SourceLocation::try_from_ast(node, id_paths)),
            Some(sources),
        )
    }
    /// Returns the `runtimeCode` code usage error.
    pub fn error_runtime_code(
        node: Option<&str>,
        id_paths: &BTreeMap<usize, &String>,
        sources: &BTreeMap<String, SolcStandardJsonInputSource>,
    ) -> Self {
        let message = r#"
Deploy and runtime code are merged in PVM, accessing `type(T).runtimeCode` is not possible.
Please consider changing the functionality relying on reading runtime code to a different approach.
"#;

        Self::new_error(
            message,
            node.and_then(|node| SourceLocation::try_from_ast(node, id_paths)),
            Some(sources),
        )
    }

    /// Appends the contract path to the message..
    pub fn push_contract_path(&mut self, path: &str) {
        self.formatted_message
            .push_str(format!("\n--> {path}\n").as_str());
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.formatted_message)
    }
}
