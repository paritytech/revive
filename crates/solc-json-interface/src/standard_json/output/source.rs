//! The `solc --standard-json` output source.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

use crate::standard_json::output::error::Error as SolcStandardJsonOutputError;
#[cfg(feature = "resolc")]
use crate::warning::Warning;
use crate::SolcStandardJsonInputSource;

/// The `solc --standard-json` output source.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// The source code ID.
    pub id: usize,
    /// The source code AST.
    pub ast: Option<serde_json::Value>,
}

impl Source {
    /// Checks the AST node for the `ecrecover` function usage.
    pub fn check_ecrecover(ast: &serde_json::Value) -> Option<SolcStandardJsonOutputError> {
        let ast = ast.as_object()?;

        if ast.get("nodeType")?.as_str()? != "FunctionCall" {
            return None;
        }

        let expression = ast.get("expression")?.as_object()?;
        if expression.get("nodeType")?.as_str()? != "Identifier" {
            return None;
        }
        if expression.get("name")?.as_str()? != "ecrecover" {
            return None;
        }

        Some(SolcStandardJsonOutputError::message_ecrecover(
            ast.get("src")?.as_str(),
        ))
    }

    /// Checks the AST node for the `<address payable>`'s `send` and `transfer` methods usage.
    pub fn check_send_and_transfer(
        ast: &serde_json::Value,
        id_paths: &BTreeMap<usize, &String>,
        sources: &BTreeMap<String, SolcStandardJsonInputSource>,
    ) -> Option<SolcStandardJsonOutputError> {
        let ast = ast.as_object()?;

        (ast.get("nodeType")?.as_str()? == "FunctionCall").then_some(())?;

        let expression = ast.get("expression")?.as_object()?;
        (expression.get("nodeType")?.as_str()? == "MemberAccess").then_some(())?;
        let member_name = expression.get("memberName")?.as_str()?;
        ["send", "transfer"].contains(&member_name).then_some(())?;

        let expression = expression.get("expression")?.as_object()?;
        let type_descriptions = expression.get("typeDescriptions")?.as_object()?;
        let type_identifier = type_descriptions.get("typeIdentifier")?.as_str()?;
        let mut affected_types = vec!["t_address_payable"];
        affected_types.contains(&type_identifier).then_some(())?;

        Some(SolcStandardJsonOutputError::error_send_and_transfer(
            ast.get("src")?.as_str(),
            id_paths,
            sources,
        ))
    }

    /// Checks the AST node for the `extcodesize` assembly instruction usage.
    pub fn check_assembly_extcodesize(
        ast: &serde_json::Value,
    ) -> Option<SolcStandardJsonOutputError> {
        let ast = ast.as_object()?;

        if ast.get("nodeType")?.as_str()? != "YulFunctionCall" {
            return None;
        }
        if ast
            .get("functionName")?
            .as_object()?
            .get("name")?
            .as_str()?
            != "extcodesize"
        {
            return None;
        }

        Some(SolcStandardJsonOutputError::message_extcodesize(
            ast.get("src")?.as_str(),
        ))
    }

    /// Checks the AST node for the `origin` assembly instruction usage.
    pub fn check_assembly_origin(ast: &serde_json::Value) -> Option<SolcStandardJsonOutputError> {
        let ast = ast.as_object()?;

        if ast.get("nodeType")?.as_str()? != "YulFunctionCall" {
            return None;
        }
        if ast
            .get("functionName")?
            .as_object()?
            .get("name")?
            .as_str()?
            != "origin"
        {
            return None;
        }

        Some(SolcStandardJsonOutputError::message_tx_origin(
            ast.get("src")?.as_str(),
        ))
    }

    /// Checks the AST node for the `tx.origin` value usage.
    pub fn check_tx_origin(ast: &serde_json::Value) -> Option<SolcStandardJsonOutputError> {
        let ast = ast.as_object()?;

        if ast.get("nodeType")?.as_str()? != "MemberAccess" {
            return None;
        }
        if ast.get("memberName")?.as_str()? != "origin" {
            return None;
        }

        let expression = ast.get("expression")?.as_object()?;
        if expression.get("nodeType")?.as_str()? != "Identifier" {
            return None;
        }
        if expression.get("name")?.as_str()? != "tx" {
            return None;
        }

        Some(SolcStandardJsonOutputError::message_tx_origin(
            ast.get("src")?.as_str(),
        ))
    }

    /// Returns the list of messages for some specific parts of the AST.
    #[cfg(feature = "resolc")]
    pub fn get_messages(
        ast: &serde_json::Value,
        id_paths: &BTreeMap<usize, &String>,
        sources: &BTreeMap<String, SolcStandardJsonInputSource>,
        suppressed_warnings: &[StandardJsonInputSettingsWarningType],
    ) -> Vec<SolcStandardJsonOutputError> {
        let mut messages = Vec::new();
        if !suppressed_warnings.contains(&Warning::EcRecover) {
            if let Some(message) = Self::check_ecrecover(ast) {
                messages.push(message);
            }
        }
        if !suppressed_warnings.contains(&Warning::SendTransfer) {
            if let Some(message) = Self::check_send_and_transfer(ast) {
                messages.push(message);
            }
        }
        if !suppressed_warnings.contains(&Warning::ExtCodeSize) {
            if let Some(message) = Self::check_assembly_extcodesize(ast) {
                messages.push(message);
            }
        }
        if !suppressed_warnings.contains(&Warning::TxOrigin) {
            if let Some(message) = Self::check_assembly_origin(ast) {
                messages.push(message);
            }
            if let Some(message) = Self::check_tx_origin(ast) {
                messages.push(message);
            }
        }

        match ast {
            serde_json::Value::Array(array) => {
                for element in array.iter() {
                    messages.extend(Self::get_messages(element, suppressed_warnings));
                }
            }
            serde_json::Value::Object(object) => {
                for (_key, value) in object.iter() {
                    messages.extend(Self::get_messages(value, suppressed_warnings));
                }
            }
            _ => {}
        }

        messages
    }

    /// Returns the name of the last contract.
    pub fn last_contract_name(&self) -> anyhow::Result<String> {
        self.ast
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("The AST is empty"))?
            .get("nodes")
            .and_then(|value| value.as_array())
            .ok_or_else(|| {
                anyhow::anyhow!("The last contract cannot be found in an empty list of nodes")
            })?
            .iter()
            .filter_map(
                |node| match node.get("nodeType").and_then(|node| node.as_str()) {
                    Some("ContractDefinition") => Some(node.get("name")?.as_str()?.to_owned()),
                    _ => None,
                },
            )
            .next_back()
            .ok_or_else(|| anyhow::anyhow!("The last contract not found in the AST"))
    }
}
