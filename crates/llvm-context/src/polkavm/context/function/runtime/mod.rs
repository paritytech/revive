//! The front-end runtime functions.

pub mod deploy_code;
pub mod entry;
pub mod runtime_code;

use crate::polkavm::context::address_space::AddressSpace;

/// The front-end runtime functions.
#[derive(Debug, Clone)]
pub struct Runtime {
    /// The address space where the calldata is allocated.
    /// Solidity uses the ordinary heap. Vyper uses the auxiliary heap.
    _address_space: AddressSpace,
}

impl Runtime {
    /// The main entry function name.
    pub const FUNCTION_ENTRY: &'static str = "__entry";

    /// The deploy code function name.
    pub const FUNCTION_DEPLOY_CODE: &'static str = "__deploy";

    /// The runtime code function name.
    pub const FUNCTION_RUNTIME_CODE: &'static str = "__runtime";

    /// A shortcut constructor.
    pub fn new(_address_space: AddressSpace) -> Self {
        Self { _address_space }
    }
}
