//! The front-end runtime functions.

pub mod arithmetics;
pub mod deploy_code;
pub mod entry;
pub mod revive;
pub mod runtime_code;
pub mod sbrk;

/// The main entry function name.
pub const FUNCTION_ENTRY: &str = "__entry";

/// The deploy code function name.
pub const FUNCTION_DEPLOY_CODE: &str = "__deploy";

/// The runtime code function name.
pub const FUNCTION_RUNTIME_CODE: &str = "__runtime";
