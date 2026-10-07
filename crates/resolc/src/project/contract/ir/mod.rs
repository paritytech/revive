//! The contract source code.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use serde::Deserialize;
use serde::Serialize;

use self::newyork::NewYork;
use self::yul::Yul;

pub mod newyork;
pub mod yul;

/// The contract source code.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(clippy::upper_case_acronyms)]
pub enum IR {
    /// The Yul source code.
    Yul(Yul),
    /// The newyork IR (via Yul translation).
    NewYork(NewYork),
}

impl IR {
    /// Whether this contract is lowered through the newyork IR pipeline.
    pub fn is_newyork(&self) -> bool {
        matches!(self, Self::NewYork(_))
    }

    /// Drains the list of factory dependencies.
    pub fn drain_factory_dependencies(&mut self) -> BTreeSet<String> {
        match self {
            IR::Yul(ref mut yul) => std::mem::take(&mut yul.object.factory_dependencies),
            IR::NewYork(ref mut newyork) => {
                std::mem::take(&mut newyork.yul_object.factory_dependencies)
            }
        }
        .into_keys()
        .collect()
    }

    /// Returns the nested contract objects missing in `identifier_paths`, by identifier.
    ///
    /// Only objects with their own `_deployed` runtime object are contracts, as solc emits them.
    pub fn unresolved_factory_dependencies(
        &self,
        identifier_paths: &BTreeMap<String, String>,
    ) -> Vec<(String, Self)> {
        let factory_dependencies = match self {
            IR::Yul(yul) => &yul.object.factory_dependencies,
            IR::NewYork(newyork) => &newyork.yul_object.factory_dependencies,
        };
        factory_dependencies
            .iter()
            .filter(|(identifier, object)| {
                object.explicit_runtime_code && !identifier_paths.contains_key(identifier.as_str())
            })
            .map(|(identifier, object)| {
                let ir = match self {
                    IR::Yul(_) => Self::Yul(Yul {
                        object: object.to_owned(),
                    }),
                    IR::NewYork(_) => Self::NewYork(NewYork {
                        yul_object: object.to_owned(),
                    }),
                };
                (identifier.to_owned(), ir)
            })
            .collect()
    }

    /// Get the list of missing deployable libraries.
    pub fn get_missing_libraries(&self) -> BTreeSet<String> {
        match self {
            Self::Yul(inner) => inner.get_missing_libraries(),
            Self::NewYork(inner) => inner.get_missing_libraries(),
        }
    }
}

impl From<Yul> for IR {
    fn from(inner: Yul) -> Self {
        Self::Yul(inner)
    }
}

impl From<NewYork> for IR {
    fn from(inner: NewYork) -> Self {
        Self::NewYork(inner)
    }
}
