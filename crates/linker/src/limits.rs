//! Deployment limit checks for linked PVM blobs.
//!
//! `pallet_revive` rejects a contract at deployment time if its blob is too large, contains a
//! basic block that is too long, or needs more interpreter memory than the runtime is willing to
//! hold. Those checks live in `pallet_revive::limits::code::enforce`. Failing them means the
//! contract compiles fine but can never be deployed, which is a poor thing to find out after the
//! fact, so this module reproduces them at link time.
//!
//! The estimates are not approximated: the pallet derives its memory numbers from
//! [`ProgramBlob::estimate_interpreter_memory_usage`], and so do we, from the same crate and with
//! the same arguments. What the compiler cannot know is which runtime the blob is destined for,
//! hence [`PalletLimits`] is configurable and merely defaults to Asset Hub.

use polkavm_common::program::{
    EstimateInterpreterMemoryUsageArgs, ISA_ReviveV1, InstructionSetKind, ProgramBlob,
};

/// The deployment limits of a target runtime.
///
/// Mirrors the constants in `pallet_revive::limits`. Different chains, or the same chain at a
/// different runtime version, can carry different values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PalletLimits {
    /// `limits::code::BLOB_BYTES`: the maximum length of a code blob.
    pub blob_bytes: u32,
    /// `limits::code::BASIC_BLOCK_SIZE`: the maximum basic block length in instructions.
    pub basic_block_size: u32,
    /// `limits::code::INTERPRETER_CACHE_BYTES`: the interpreter compilation artifact budget.
    pub interpreter_cache_bytes: u32,
    /// `limits::code::PURGABLE_MEMORY_LIMIT`: memory that is dropped when calling out.
    pub purgable_memory_limit: u32,
    /// `limits::code::BASELINE_MEMORY_LIMIT`: memory held for the contract's whole life time.
    pub baseline_memory_limit: u32,
    /// `limits::PAGE_SIZE`.
    pub page_size: u32,
}

impl PalletLimits {
    /// The limits of the Asset Hub runtime.
    pub const ASSET_HUB: Self = Self {
        blob_bytes: 1024 * 1024,
        basic_block_size: 1000,
        interpreter_cache_bytes: 1024 * 1024,
        purgable_memory_limit: 1024 * 1024 + 2 * 1024 * 1024,
        baseline_memory_limit: 1024 * 1024 + 512 * 1024,
        page_size: 4 * 1024,
    };
}

impl Default for PalletLimits {
    fn default() -> Self {
        Self::ASSET_HUB
    }
}

/// How the compiler reacts to a blob that would be rejected at deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Enforcement {
    /// Fail the compilation. A contract that cannot be deployed is not a useful artifact.
    #[default]
    Deny,
    /// Report the violation but emit the contract anyway.
    Warn,
}

/// The deployment limit configuration a build is checked against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeploymentLimits {
    /// The limits of the target runtime.
    pub limits: PalletLimits,
    /// What to do when they are exceeded.
    pub enforcement: Enforcement,
}

impl DeploymentLimits {
    /// A shorthand constructor.
    pub fn new(baseline_memory_limit: u32, enforcement: Enforcement) -> Self {
        Self {
            limits: PalletLimits {
                baseline_memory_limit,
                ..PalletLimits::ASSET_HUB
            },
            enforcement,
        }
    }
}

/// A reason the linked blob would be rejected at deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Violation {
    /// The blob is longer than the runtime accepts.
    BlobTooLarge { size: u32, limit: u32 },
    /// A basic block exceeds the maximum instruction count.
    BasicBlockTooLarge { size: u32, limit: u32 },
    /// The interpreter would need too much purgeable memory.
    PurgeableMemoryTooLarge { size: u32, limit: u32 },
    /// The interpreter would need too much memory for the contract's whole life time.
    BaselineMemoryTooLarge { size: u32, limit: u32 },
}

impl core::fmt::Display for Violation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BlobTooLarge { size, limit } => write!(
                f,
                "the contract code is {size} bytes but the runtime accepts at most {limit} bytes"
            ),
            Self::BasicBlockTooLarge { size, limit } => write!(
                f,
                "the contract contains a basic block of {size} instructions but the runtime \
                 accepts at most {limit}"
            ),
            Self::PurgeableMemoryTooLarge { size, limit } => write!(
                f,
                "the contract needs {size} bytes of purgeable interpreter memory but the runtime \
                 provides at most {limit} bytes"
            ),
            Self::BaselineMemoryTooLarge { size, limit } => write!(
                f,
                "the contract needs {size} bytes of baseline interpreter memory but the runtime \
                 provides at most {limit} bytes; the static heap and stack buffers are part of \
                 this budget, so lowering `--heap-size` or `--stack-size` frees room for code"
            ),
        }
    }
}

/// What the runtime would observe about a linked blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobFootprint {
    /// The blob length in bytes.
    pub size: u32,
    /// The longest basic block, in instructions.
    pub max_basic_block_size: u32,
    /// Interpreter memory that is dropped when the contract calls out.
    pub purgeable_ram_consumption: u32,
    /// Interpreter memory held for the contract's whole life time.
    pub baseline_ram_consumption: u32,
}

impl BlobFootprint {
    /// Measures `blob` the way `pallet_revive::limits::code::enforce` does.
    ///
    /// `interpreter_cache_bytes` and `page_size` come from the target runtime, because the
    /// interpreter memory estimate depends on them.
    pub fn measure(
        blob: &[u8],
        interpreter_cache_bytes: u32,
        page_size: u32,
    ) -> anyhow::Result<Self> {
        let size = u32::try_from(blob.len())
            .map_err(|_| anyhow::anyhow!("the linked blob does not fit into a u32"))?;

        let program = ProgramBlob::parse(blob.into())
            .map_err(|error| anyhow::anyhow!("failed to parse the linked blob: {error}"))?;

        anyhow::ensure!(
            program.isa() == InstructionSetKind::ReviveV1,
            "the linked blob targets the '{}' instruction set instead of '{}'",
            program.isa().name(),
            InstructionSetKind::ReviveV1.name(),
        );

        // The same single pass the pallet does: the interpreter memory estimate needs the
        // instruction and basic block counts, and the longest basic block is a limit of its own.
        let mut max_basic_block_size = 0u32;
        let mut basic_block_size = 0u32;
        let mut basic_block_count = 0u32;
        let mut instruction_count = 0u32;
        for instruction in program.instructions_with_isa(ISA_ReviveV1) {
            basic_block_size += 1;
            instruction_count += 1;
            if instruction.kind.opcode().starts_new_basic_block() {
                max_basic_block_size = max_basic_block_size.max(basic_block_size);
                basic_block_size = 0;
                basic_block_count += 1;
            }
        }
        max_basic_block_size = max_basic_block_size.max(basic_block_size);

        let usage = program
            .estimate_interpreter_memory_usage(EstimateInterpreterMemoryUsageArgs::BoundedCache {
                max_cache_size_bytes: interpreter_cache_bytes,
                instruction_count,
                max_block_size: max_basic_block_size,
                basic_block_count,
                page_size,
            })
            .map_err(|error| {
                anyhow::anyhow!("failed to estimate the interpreter memory usage: {error}")
            })?;

        Ok(Self {
            size,
            max_basic_block_size,
            purgeable_ram_consumption: usage.purgeable_ram_consumption,
            baseline_ram_consumption: usage.baseline_ram_consumption,
        })
    }

    /// Returns every limit this footprint exceeds, in the order the pallet checks them.
    pub fn violations(&self, limits: &PalletLimits) -> Vec<Violation> {
        let mut violations = Vec::new();

        if self.size > limits.blob_bytes {
            violations.push(Violation::BlobTooLarge {
                size: self.size,
                limit: limits.blob_bytes,
            });
        }
        if self.max_basic_block_size > limits.basic_block_size {
            violations.push(Violation::BasicBlockTooLarge {
                size: self.max_basic_block_size,
                limit: limits.basic_block_size,
            });
        }
        if self.purgeable_ram_consumption > limits.purgable_memory_limit {
            violations.push(Violation::PurgeableMemoryTooLarge {
                size: self.purgeable_ram_consumption,
                limit: limits.purgable_memory_limit,
            });
        }
        if self.baseline_ram_consumption > limits.baseline_memory_limit {
            violations.push(Violation::BaselineMemoryTooLarge {
                size: self.baseline_ram_consumption,
                limit: limits.baseline_memory_limit,
            });
        }

        violations
    }
}

/// Measures `blob` and returns the limits it exceeds for the given runtime.
pub fn check(blob: &[u8], limits: &PalletLimits) -> anyhow::Result<Vec<Violation>> {
    let footprint = BlobFootprint::measure(blob, limits.interpreter_cache_bytes, limits.page_size)?;

    Ok(footprint.violations(limits))
}
