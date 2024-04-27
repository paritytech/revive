//!
//! The LLVM target.
//!

use std::str::FromStr;

///
/// The LLVM target.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The EraVM target.
    PVM,
}

impl Target {
    ///
    /// Returns the target name.
    ///
    pub fn name(&self) -> &str {
        match self {
            Self::PVM => "riscv64",
        }
    }

    ///
    /// Returns the target triple.
    ///
    pub fn triple(&self) -> &str {
        match self {
            Self::PVM => "riscv64-unknown-unknown-elf",
        }
    }

    ///
    /// Returns the target production name.
    ///
    pub fn production_name(&self) -> &str {
        match self {
            Self::PVM => "PVM",
        }
    }
}

impl FromStr for Target {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "riscv64" => Ok(Self::PVM),
            _ => Err(anyhow::anyhow!(
                "Unknown target `{}`. Supported targets: {:?}",
                string,
                Self::PVM
            )),
        }
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::PVM => write!(f, "riscv64"),
        }
    }
}
