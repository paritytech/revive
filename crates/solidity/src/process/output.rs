//!
//! Process for compiling a single compilation unit.
//!
//! The output data.
//!

use serde::Deserialize;
use serde::Serialize;

use crate::build::contract::Contract as ContractBuild;

///
/// The output data.
///
#[derive(Debug, Serialize, Deserialize)]
pub struct Output {
    /// The contract build.
    pub build: ContractBuild,
}

impl Output {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(build: ContractBuild) -> Self {
        Self { build }
    }
}
