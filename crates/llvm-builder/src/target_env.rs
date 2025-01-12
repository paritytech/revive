//! The target environments to build LLVM.

/// The list of target environments used as constants.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetEnv {
    /// The GNU target environment.
    #[default]
    GNU,
    /// The MUSL target environment.
    MUSL,
    /// The wasm32 Emscripten environment.
    Emscripten,
}

impl std::str::FromStr for TargetEnv {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "gnu" => Ok(Self::GNU),
            "musl" => Ok(Self::MUSL),
            "emscripten" => Ok(Self::Emscripten),
            value => Err(format!("Unsupported target environment: `{}`", value)),
        }
    }
}

impl std::fmt::Display for TargetEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GNU => write!(f, "gnu"),
            Self::MUSL => write!(f, "musl"),
            Self::Emscripten => write!(f, "emscripten"),
        }
    }
}
