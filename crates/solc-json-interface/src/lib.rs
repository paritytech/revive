//! This crates provides a typed and serializable Rust interface for
//! interacting with the `solc` standard JSON and combined JSON modes.

pub use self::combined_json::contract::Contract as CombinedJsonContract;
pub use self::standard_json::input::language::Language as SolcStandardJsonInputLanguage;
pub use self::standard_json::input::settings::metadata::Metadata as SolcStandardJsonInputSettingsMetadata;
pub use self::standard_json::input::settings::metadata_hash::MetadataHash as SolcStandardJsonInputSettingsMetadataHash;
pub use self::standard_json::input::settings::optimizer::Optimizer as SolcStandardJsonInputSettingsOptimizer;
pub use self::standard_json::input::settings::selection::file::flag::Flag as SolcStandardJsonInputSettingsSelectionFileFlag;
pub use self::standard_json::input::settings::selection::file::File as SolcStandardJsonInputSettingsSelectionFile;
pub use self::standard_json::input::settings::selection::Selection as SolcStandardJsonInputSettingsSelection;
pub use self::standard_json::input::settings::Settings as SolcStandardJsonInputSettings;
pub use self::standard_json::input::source::Source as SolcStandardJsonInputSource;
pub use self::standard_json::input::Input as SolcStandardJsonInput;
pub use self::standard_json::output::contract::evm::bytecode::Bytecode as SolcStandardJsonOutputContractEVMBytecode;
pub use self::standard_json::output::contract::evm::EVM as SolcStandardJsonOutputContractEVM;
pub use self::standard_json::output::contract::Contract as SolcStandardJsonOutputContract;
pub use self::standard_json::output::Output as SolcStandardJsonOutput;
#[cfg(feature = "resolc")]
pub use self::warning::Warning as ResolcWarning;

pub mod combined_json;
pub mod standard_json;
#[cfg(feature = "resolc")]
pub mod warning;
