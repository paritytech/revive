//! Solidity to PolkaVM compiler arguments.

use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use clap::Parser;
use path_slash::PathExt;

/// Compiles the provided Solidity input files (or use the standard input if no files
/// are given or "-" is specified as a file name). Outputs the components based on the
/// chosen options, either to the standard output or to files within the designated
/// output directory.
/// Example: resolc ERC20.sol -O3 --bin --output-dir './build/'
#[derive(Debug, Parser)]
#[structopt(name = "The PolkaVM Solidity compiler")]
pub struct Arguments {
    /// Print the version and exit.
    #[structopt(long = "version")]
    pub version: bool,

    /// Print the licence and exit.
    #[structopt(long = "license")]
    pub license: bool,

    /// Specify the input paths and remappings.
    /// If an argument contains a '=', it is considered a remapping.
    /// Multiple Solidity files can be passed in the default Solidity mode.
    /// Yul, LLVM IR, and PolkaVM Assembly modes currently support only a single file.
    pub inputs: Vec<String>,

    /// Set the given path as the root of the source tree instead of the root of the filesystem.
    /// Passed to `solc` without changes.
    #[structopt(long = "base-path")]
    pub base_path: Option<String>,

    /// Make an additional source directory available to the default import callback.
    /// Can be used multiple times. Can only be used if the base path has a non-empty value.
    /// Passed to `solc` without changes.
    #[structopt(long = "include-path")]
    pub include_paths: Vec<String>,

    /// Allow a given path for imports. A list of paths can be supplied by separating them with a comma.
    /// Passed to `solc` without changes.
    #[structopt(long = "allow-paths")]
    pub allow_paths: Option<String>,

    /// Create one file per component and contract/file at the specified directory, if given.
    #[structopt(short = 'o', long = "output-dir")]
    pub output_directory: Option<PathBuf>,

    /// Overwrite existing files (used together with -o).
    #[structopt(long = "overwrite")]
    pub overwrite: bool,

    /// Set the optimization parameter -O[0 | 1 | 2 | 3 | s | z].
    /// Use `3` for best performance and `z` for minimal size.
    #[structopt(short = 'O', long = "optimization")]
    pub optimization: Option<char>,

    /// Try to recompile with -Oz if the bytecode is too large.
    #[structopt(long = "fallback-Oz")]
    pub fallback_to_optimizing_for_size: bool,

    /// Disable the system request memoization.
    #[structopt(long = "disable-system-request-memoization")]
    pub disable_system_request_memoization: bool,

    /// Disable the `solc` optimizer.
    /// Use it if your project uses the `MSIZE` instruction, or in other cases.
    /// Beware that it will prevent libraries from being inlined.
    #[structopt(long = "disable-solc-optimizer")]
    pub disable_solc_optimizer: bool,

    /// Specify the path to the `solc` executable. By default, the one in `${PATH}` is used.
    /// Yul mode: `solc` is used for source code validation, as `resolc` itself assumes that the input Yul is valid.
    /// LLVM IR mode: `solc` is unused.
    #[structopt(long = "solc")]
    pub solc: Option<String>,

    /// The EVM target version to generate IR for.
    /// See https://github.com/paritytech/revive/blob/main/crates/common/src/evm_version.rs for reference.
    #[structopt(long = "evm-version")]
    pub evm_version: Option<String>,

    /// Specify addresses of deployable libraries. Syntax: `<libraryName>=<address> [, or whitespace] ...`.
    /// Addresses are interpreted as hexadecimal strings prefixed with `0x`.
    #[structopt(short = 'l', long = "libraries")]
    pub libraries: Vec<String>,

    /// Output a single JSON document containing the specified information.
    /// Available arguments: `abi`, `hashes`, `metadata`, `devdoc`, `userdoc`, `storage-layout`, `ast`, `asm`, `bin`, `bin-runtime`.
    #[structopt(long = "combined-json")]
    pub combined_json: Option<String>,

    /// Switch to standard JSON input/output mode. Read from stdin, write the result to stdout.
    /// This is the default used by the Hardhat plugin.
    #[structopt(long = "standard-json")]
    pub standard_json: bool,

    /// Switch to missing deployable libraries detection mode.
    /// Only available for standard JSON input/output mode.
    /// Contracts are not compiled in this mode, and all compilation artifacts are not included.
    #[structopt(long = "detect-missing-libraries")]
    pub detect_missing_libraries: bool,

    /// Switch to Yul mode.
    /// Only one input Yul file is allowed.
    /// Cannot be used with combined and standard JSON modes.
    #[structopt(long = "yul")]
    pub yul: bool,

    /// Switch to LLVM IR mode.
    /// Only one input LLVM IR file is allowed.
    /// Cannot be used with combined and standard JSON modes.
    /// Use this mode at your own risk, as LLVM IR input validation is not implemented.
    #[structopt(long = "llvm-ir")]
    pub llvm_ir: bool,

    /// Forcibly switch to EVM legacy assembly pipeline.
    /// It is useful for older revisions of `solc` 0.8, where Yul was considered highly experimental
    /// and contained more bugs than today.
    #[structopt(long = "force-evmla")]
    pub force_evmla: bool,

    /// Enable system contract compilation mode.
    /// In this mode PolkaVM extensions are enabled. For example, calls to addresses `0xFFFF` and below
    /// are substituted by special PolkaVM instructions.
    /// In the Yul mode, the `verbatim_*` instruction family is available.
    #[structopt(long = "system-mode")]
    pub is_system_mode: bool,

    /// Set metadata hash mode.
    /// The only supported value is `none` that disables appending the metadata hash.
    /// Is enabled by default.
    #[structopt(long = "metadata-hash")]
    pub metadata_hash: Option<String>,

    /// Output PolkaVM assembly of the contracts.
    #[structopt(long = "asm")]
    pub output_assembly: bool,

    /// Output PolkaVM bytecode of the contracts.
    #[structopt(long = "bin")]
    pub output_binary: bool,

    /// Suppress specified warnings.
    /// Available arguments: `ecrecover`, `sendtransfer`, `extcodesize`, `txorigin`, `blocktimestamp`, `blocknumber`, `blockhash`.
    #[structopt(long = "suppress-warnings")]
    pub suppress_warnings: Option<Vec<String>>,

    /// Generate source based debug information in the output code file. This only has an effect
    /// with the LLVM-IR code generator and is ignored otherwise.
    #[structopt(short = 'g')]
    pub emit_source_debug_info: bool,

    /// Dump all IRs to files in the specified directory.
    /// Only for testing and debugging.
    #[structopt(long = "debug-output-dir")]
    pub debug_output_directory: Option<PathBuf>,

    /// Set the verify-each option in LLVM.
    /// Only for testing and debugging.
    #[structopt(long = "llvm-verify-each")]
    pub llvm_verify_each: bool,

    /// Set the debug-logging option in LLVM.
    /// Only for testing and debugging.
    #[structopt(long = "llvm-debug-logging")]
    pub llvm_debug_logging: bool,

    /// Run this process recursively and provide JSON input to compile a single contract.
    /// Only for usage from within the compiler.
    #[structopt(long = "recursive-process")]
    pub recursive_process: bool,

    /// Specify the input file to use instead of stdin when --recursive-process is given.
    /// This is only intended for use when developing the compiler.
    #[cfg(debug_assertions)]
    #[structopt(long = "recursive-process-input")]
    pub recursive_process_input: Option<String>,
}

impl Default for Arguments {
    fn default() -> Self {
        Self::new()
    }
}

impl Arguments {
    /// A shortcut constructor.
    pub fn new() -> Self {
        Self::parse()
    }

    /// Validates the arguments.
    #[allow(clippy::collapsible_if)]
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.version && std::env::args().count() > 2 {
            anyhow::bail!("No other options are allowed while getting the compiler version.");
        }

        #[cfg(debug_assertions)]
        if self.recursive_process_input.is_some() && !self.recursive_process {
            anyhow::bail!("--process-input can be only used when --recursive-process is given");
        }

        #[cfg(debug_assertions)]
        if self.recursive_process
            && ((self.recursive_process_input.is_none() && std::env::args().count() > 2)
                || (self.recursive_process_input.is_some() && std::env::args().count() > 4))
        {
            anyhow::bail!("No other options are allowed in recursive mode.");
        }

        #[cfg(not(debug_assertions))]
        if self.recursive_process && std::env::args().count() > 2 {
            anyhow::bail!("No other options are allowed in recursive mode.");
        }

        let modes_count = [
            self.yul,
            self.llvm_ir,
            self.combined_json.is_some(),
            self.standard_json,
        ]
        .iter()
        .filter(|&&x| x)
        .count();
        if modes_count > 1 {
            anyhow::bail!("Only one modes is allowed at the same time: Yul, LLVM IR, PolkaVM assembly, combined JSON, standard JSON.");
        }

        if self.yul || self.llvm_ir {
            if self.base_path.is_some() {
                anyhow::bail!(
                    "`base-path` is not used in Yul, LLVM IR and PolkaVM assembly modes."
                );
            }
            if !self.include_paths.is_empty() {
                anyhow::bail!(
                    "`include-paths` is not used in Yul, LLVM IR and PolkaVM assembly modes."
                );
            }
            if self.allow_paths.is_some() {
                anyhow::bail!(
                    "`allow-paths` is not used in Yul, LLVM IR and PolkaVM assembly modes."
                );
            }
            if !self.libraries.is_empty() {
                anyhow::bail!(
                    "Libraries are not supported in Yul, LLVM IR and PolkaVM assembly modes."
                );
            }

            if self.evm_version.is_some() {
                anyhow::bail!(
                    "`evm-version` is not used in Yul, LLVM IR and PolkaVM assembly modes."
                );
            }

            if self.force_evmla {
                anyhow::bail!("EVM legacy assembly mode is not supported in Yul, LLVM IR and PolkaVM assembly modes.");
            }

            if self.disable_solc_optimizer {
                anyhow::bail!("Disabling the solc optimizer is not supported in Yul, LLVM IR and PolkaVM assembly modes.");
            }
        }

        if self.llvm_ir {
            if self.solc.is_some() {
                anyhow::bail!("`solc` is not used in LLVM IR and PolkaVM assembly modes.");
            }

            if self.is_system_mode {
                anyhow::bail!(
                    "System contract mode is not supported in LLVM IR and PolkaVM assembly modes."
                );
            }
        }

        if self.combined_json.is_some() {
            if self.output_assembly || self.output_binary {
                anyhow::bail!(
                    "Cannot output assembly or binary outside of JSON in combined JSON mode."
                );
            }
        }

        if self.standard_json {
            if self.output_assembly || self.output_binary {
                anyhow::bail!(
                    "Cannot output assembly or binary outside of JSON in standard JSON mode."
                );
            }

            if !self.inputs.is_empty() {
                anyhow::bail!("Input files must be passed via standard JSON input.");
            }
            if !self.libraries.is_empty() {
                anyhow::bail!("Libraries must be passed via standard JSON input.");
            }
            if self.evm_version.is_some() {
                anyhow::bail!("EVM version must be passed via standard JSON input.");
            }

            if self.output_directory.is_some() {
                anyhow::bail!("Output directory cannot be used in standard JSON mode.");
            }
            if self.overwrite {
                anyhow::bail!("Overwriting flag cannot be used in standard JSON mode.");
            }
            if self.disable_solc_optimizer {
                anyhow::bail!(
                    "Disabling the solc optimizer must specified in standard JSON input settings."
                );
            }
            if self.optimization.is_some() {
                anyhow::bail!("LLVM optimizations must specified in standard JSON input settings.");
            }
            if self.fallback_to_optimizing_for_size {
                anyhow::bail!(
                    "Falling back to -Oz must specified in standard JSON input settings."
                );
            }
            if self.disable_system_request_memoization {
                anyhow::bail!(
                    "Disabling the system request memoization must specified in standard JSON input settings."
                );
            }
            if self.metadata_hash.is_some() {
                anyhow::bail!("Metadata hash mode must specified in standard JSON input settings.");
            }
        }

        Ok(())
    }

    /// Returns remappings from input paths.
    pub fn split_input_files_and_remappings(
        &self,
    ) -> anyhow::Result<(Vec<PathBuf>, Option<BTreeSet<String>>)> {
        let mut input_files = Vec::with_capacity(self.inputs.len());
        let mut remappings = BTreeSet::new();

        for input in self.inputs.iter() {
            if input.contains('=') {
                let mut parts = Vec::with_capacity(2);
                for path in input.trim().split('=') {
                    let path = PathBuf::from(path);
                    parts.push(
                        Self::path_to_posix(path.as_path())?
                            .to_string_lossy()
                            .to_string(),
                    );
                }
                if parts.len() != 2 {
                    anyhow::bail!(
                        "Invalid remapping `{}`: expected two parts separated by '='",
                        input
                    );
                }
                remappings.insert(parts.join("="));
            } else {
                let path = PathBuf::from(input.trim());
                let path = Self::path_to_posix(path.as_path())?;
                input_files.push(path);
            }
        }

        let remappings = if remappings.is_empty() {
            None
        } else {
            Some(remappings)
        };

        Ok((input_files, remappings))
    }

    /// Normalizes an input path by converting it to POSIX format.
    fn path_to_posix(path: &Path) -> anyhow::Result<PathBuf> {
        let path = path
            .to_slash()
            .ok_or_else(|| anyhow::anyhow!("Input path {:?} POSIX conversion error", path))?
            .to_string();
        let path = PathBuf::from(path.as_str());
        Ok(path)
    }
}
