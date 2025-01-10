//! The PolkaVM LLVM target triples.

/// The list of target triples used as constants.
///
/// It must be in the lowercase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetTriple {
    /// The PolkaVM RISC-V target triple.
    PolkaVM,
}

impl std::str::FromStr for TargetTriple {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "polkavm" => Ok(Self::PolkaVM),
            value => Err(format!("Unsupported target triple: `{}`", value)),
        }
    }
}

impl std::fmt::Display for TargetTriple {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::PolkaVM => write!(f, "riscv64-unknown-elf"),
        }
    }
}
