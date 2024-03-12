//!
//! The contract LLVM IR source code.
//!

use serde::Deserialize;
use serde::Serialize;

///
/// The contract LLVM IR source code.
///
#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(clippy::upper_case_acronyms)]
pub struct LLVMIR {
    /// The LLVM IR file path.
    pub path: String,
    /// The LLVM IR source code.
    pub source: String,
}

impl LLVMIR {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(path: String, source: String) -> Self {
        Self { path, source }
    }
}
