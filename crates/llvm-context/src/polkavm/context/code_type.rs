//! The contract code types.

/// The contract code types (deploy and runtime).
/// They do not represent any entities in the final bytecode, but this separation is always present
/// in the IRs used for translation to the EVM bytecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CodeType {
    /// The deploy code.
    Deploy,
    /// The runtime code.
    Runtime,
}

impl std::fmt::Display for CodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Deploy => write!(f, "deploy"),
            Self::Runtime => write!(f, "runtime"),
        }
    }
}
