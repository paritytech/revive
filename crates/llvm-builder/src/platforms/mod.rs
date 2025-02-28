//! The revive LLVM builder platforms.

pub mod aarch64_linux_gnu;
pub mod aarch64_linux_musl;
pub mod aarch64_macos;
pub mod shared;
pub mod wasm32_emscripten;
pub mod x86_64_linux_gnu;
pub mod x86_64_linux_musl;
pub mod x86_64_macos;
pub mod x86_64_windows_msvc;

use std::str::FromStr;

/// The list of platforms used as constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    /// The native X86 platform.
    X86,
    /// The native AArch64 platform.
    AArch64,
    /// The PolkaVM RISC-V platform.
    PolkaVM,
}

impl FromStr for Platform {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "PolkaVM" => Ok(Self::PolkaVM),
            value => Err(format!("Unsupported platform: `{}`", value)),
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::X86 => write!(f, "X86"),
            Self::AArch64 => write!(f, "AArch64"),
            Self::PolkaVM => write!(f, "RISCV"),
        }
    }
}
