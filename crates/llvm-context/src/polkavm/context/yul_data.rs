//! The LLVM IR generator Yul data.

use std::collections::BTreeMap;

/// The LLVM IR generator Yul data.
///
/// Contains data that is only relevant to Yul.
#[derive(Debug, Default)]
pub struct YulData {
    /// Mapping from Yul object identifiers to full contract paths.
    identifier_paths: BTreeMap<String, String>,
}

impl YulData {
    /// A shorthand constructor.
    pub fn new(identifier_paths: BTreeMap<String, String>) -> Self {
        Self { identifier_paths }
    }

    /// Resolves the full contract path by the Yul object identifier.
    pub fn resolve_path(&self, identifier: &str) -> Option<&str> {
        self.identifier_paths
            .get(identifier)
            .map(|path| path.as_str())
    }
}
