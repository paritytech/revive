//! The contract EraVM assembly source code.

use serde::Deserialize;
use serde::Serialize;

/// The contract EraVM assembly source code.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(clippy::upper_case_acronyms)]
pub struct ZKASM {
    /// The EraVM assembly file path.
    pub path: String,
    /// The EraVM assembly source code.
    pub source: String,
}

impl ZKASM {
    /// A shortcut constructor.
    pub fn new(path: String, source: String) -> Self {
        Self { path, source }
    }
}
