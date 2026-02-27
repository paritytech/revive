//! The contract source code.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde::Serialize;

use self::yul::Yul;

pub mod yul;

/// The contract source code.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(clippy::upper_case_acronyms)]
pub enum IR {
    /// The Yul source code.
    Yul(Yul),
}

impl IR {
    /// Drains the list of factory dependencies.
    pub fn drain_factory_dependencies(&mut self) -> BTreeSet<String> {
        match self {
            IR::Yul(ref mut yul) => yul.object.factory_dependencies.drain().collect(),
        }
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> BTreeSet<String> {
        match self {
            Self::Yul(inner) => inner.get_missing_libraries(),
        }
    }
}

impl From<Yul> for IR {
    fn from(inner: Yul) -> Self {
        Self::Yul(inner)
    }
}
