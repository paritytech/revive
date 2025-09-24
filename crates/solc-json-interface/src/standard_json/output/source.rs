//! The `solc --standard-json` output source.

#[cfg(feature = "resolc")]
use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

#[cfg(feature = "resolc")]
use crate::standard_json::input::settings::warning::Warning;
use crate::standard_json::output::error::Error as SolcStandardJsonOutputError;
#[cfg(feature = "resolc")]
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
    /// Initializes a standard JSON source.
    ///
    /// Is used for projects compiled without `solc`.
    ///
    pub fn new(id: usize) -> Self {
        Self { id, ast: None }
    }

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
        let affected_types = vec!["t_address_payable"];
        affected_types.contains(&type_identifier).then_some(())?;

        Some(Warning::SendAndTransfer.as_error(ast.get("src")?.as_str(), id_paths, sources))
    }

    /// Checks the AST node for the usage of runtime code.
    pub fn check_runtime_code(
        ast: &serde_json::Value,
        id_paths: &BTreeMap<usize, &String>,
        sources: &BTreeMap<String, SolcStandardJsonInputSource>,
    ) -> Option<SolcStandardJsonOutputError> {
        let ast = ast.as_object()?;

        (ast.get("nodeType")?.as_str()? == "MemberAccess").then_some(())?;
        (ast.get("memberName")?.as_str()? == "runtimeCode").then_some(())?;

        let expression = ast.get("expression")?.as_object()?;
        let type_descriptions = expression.get("typeDescriptions")?.as_object()?;
        type_descriptions
            .get("typeIdentifier")?
            .as_str()?
            .starts_with("t_magic_meta_type")
            .then_some(())?;

        Some(SolcStandardJsonOutputError::error_runtime_code(
            ast.get("src")?.as_str(),
            id_paths,
            sources,
        ))
    }

    /// Checks the AST node for the `tx.origin` value usage.
    pub fn check_tx_origin(
        ast: &serde_json::Value,
        id_paths: &BTreeMap<usize, &String>,
        sources: &BTreeMap<String, SolcStandardJsonInputSource>,
    ) -> Option<SolcStandardJsonOutputError> {
        let ast = ast.as_object()?;

        (ast.get("nodeType")?.as_str()? == "MemberAccess").then_some(())?;
        (ast.get("memberName")?.as_str()? == "origin").then_some(())?;

        let expression = ast.get("expression")?.as_object()?;
        (expression.get("nodeType")?.as_str()? == "Identifier").then_some(())?;
        (expression.get("name")?.as_str()? == "tx").then_some(())?;

        Some(Warning::TxOrigin.as_error(ast.get("src")?.as_str(), id_paths, sources))
    }

    /// Returns the list of messages for some specific parts of the AST.
    #[cfg(feature = "resolc")]
    pub fn get_messages(
        ast: &serde_json::Value,
        id_paths: &BTreeMap<usize, &String>,
        sources: &BTreeMap<String, SolcStandardJsonInputSource>,
        suppressed_warnings: &[Warning],
    ) -> Vec<SolcStandardJsonOutputError> {
        let mut messages = Vec::new();
        if !suppressed_warnings.contains(&Warning::SendAndTransfer) {
            if let Some(message) = Self::check_send_and_transfer(ast, id_paths, sources) {
                messages.push(message);
            }
        }
        if !suppressed_warnings.contains(&Warning::TxOrigin) {
            if let Some(message) = Self::check_tx_origin(ast, id_paths, sources) {
                messages.push(message);
            }
        }
        if let Some(message) = Self::check_runtime_code(ast, id_paths, sources) {
            messages.push(message);
        }

        match ast {
            serde_json::Value::Array(array) => {
                for element in array.iter() {
                    messages.extend(Self::get_messages(
                        element,
                        id_paths,
                        sources,
                        suppressed_warnings,
                    ));
                }
            }
            serde_json::Value::Object(object) => {
                for (_key, value) in object.iter() {
                    messages.extend(Self::get_messages(
                        value,
                        id_paths,
                        sources,
                        suppressed_warnings,
                    ));
                }
            }
            _ => {}
        }

        messages
    }
}
