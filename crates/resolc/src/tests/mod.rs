//! The Solidity compiler unit tests.

#![cfg(test)]

mod cli;
mod factory_dependency;
mod ir_artifacts;
mod libraries;
// mod messages;
mod optimizer;
mod remappings;
mod runtime_code;
mod unsupported_opcodes;

use std::collections::BTreeMap;

use revive_solc_json_interface::SolcStandardJsonInputSource;

pub(crate) use super::test_utils::*;

pub fn sources<T: ToString>(sources: &[(T, T)]) -> BTreeMap<String, SolcStandardJsonInputSource> {
    BTreeMap::from_iter(
        sources
            .iter()
            .map(|(path, code)| (path.to_string(), code.to_string().into())),
    )
}
