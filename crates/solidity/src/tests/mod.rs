//!
//! The Solidity compiler unit tests.
//!

#![cfg(test)]

mod factory_dependency;
mod ir_artifacts;
mod libraries;
mod messages;
mod optimizer;
mod remappings;
mod runtime_code;
mod unsupported_opcodes;

pub(crate) use super::test_utils::*;
