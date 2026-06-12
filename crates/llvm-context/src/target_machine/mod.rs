//! The LLVM target machine.

use crate::optimizer::settings::size_level::SizeLevel as OptimizerSettingsSizeLevel;
use crate::optimizer::settings::Settings as OptimizerSettings;

use self::target::Target;

pub mod target;

/// The LLVM target machine.
#[derive(Debug)]
pub struct TargetMachine {
    /// The LLVM target.
    target: Target,
    /// The LLVM target machine reference.
    target_machine: inkwell::targets::TargetMachine,
    /// The optimizer settings.
    optimizer_settings: OptimizerSettings,
}

impl TargetMachine {
    /// The LLVM target name.
    pub const VM_TARGET_NAME: &'static str = "riscv64";

    /// The LLVM target triple.
    pub const VM_TARGET_TRIPLE: &'static str = "riscv64-unknown-unknown-elf";

    pub const VM_TARGET_CPU: &'static str = "generic-rv64";

    /// LLVM target features.
    pub const VM_FEATURES: &'static str =
        "+e,+m,+a,+c,+zbb,+auipc-addi-fusion,+ld-add-fusion,+lui-addi-fusion,+xtheadcondmov,+relax";

    /// RISC-V backend feature that lets the code generator emit single (potentially unaligned)
    /// scalar memory accesses instead of splitting them into byte-wise sequences. Enabled for
    /// newyork modules only; the stock Yul path omits it, matching the upstream target
    /// configuration. Enabling it on the Yul path was observed to introduce an end-to-end
    /// regression (`complex/internal_function_pointers/mixed_features_2`); the exact interaction
    /// was not isolated, so the feature is restricted to the path it was validated against.
    pub const VM_FEATURE_UNALIGNED: &'static str = ",+unaligned-scalar-mem";

    /// A shortcut constructor.
    /// A separate instance for every optimization level is created.
    ///
    /// `unaligned_scalar_mem` appends [`Self::VM_FEATURE_UNALIGNED`]; pass `true` only for newyork
    /// modules (see the constant's documentation).
    pub fn new(
        target: Target,
        optimizer_settings: &OptimizerSettings,
        unaligned_scalar_mem: bool,
    ) -> anyhow::Result<Self> {
        let features = if unaligned_scalar_mem {
            format!("{}{}", Self::VM_FEATURES, Self::VM_FEATURE_UNALIGNED)
        } else {
            Self::VM_FEATURES.to_string()
        };
        let target_machine = inkwell::targets::Target::from_name(target.name())
            .ok_or_else(|| anyhow::anyhow!("LLVM target machine `{}` not found", target.name()))?
            .create_target_machine(
                &inkwell::targets::TargetTriple::create(target.triple()),
                Self::VM_TARGET_CPU,
                &features,
                optimizer_settings.level_back_end,
                inkwell::targets::RelocMode::PIC,
                inkwell::targets::CodeModel::Default,
            )
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "LLVM target machine `{}` initialization error",
                    target.name(),
                )
            })?;

        Ok(Self {
            target,
            target_machine,
            optimizer_settings: optimizer_settings.to_owned(),
        })
    }

    /// Writes the LLVM module to a memory buffer.
    pub fn write_to_memory_buffer(
        &self,
        module: &inkwell::module::Module,
    ) -> Result<inkwell::memory_buffer::MemoryBuffer<'static>, inkwell::support::LLVMString> {
        match self.target {
            Target::PVM => self
                .target_machine
                .write_to_memory_buffer(module, inkwell::targets::FileType::Object),
        }
    }

    /// Runs the optimization passes on `module`.
    ///
    /// The pass builder's `MergeFunctions` insertion is disabled for both the stock and newyork
    /// pipelines. It can collapse `@deploy` and `@call` into a single function body when their
    /// lowered control flow becomes equivalent (e.g. a bare `invalid()` in both code sections);
    /// the resulting ELF then has two exports pointing at the same block, which triggers a
    /// non-determinism bug in polkavm-linker's reachability consistency check (the exports are
    /// stored as `Vec<usize>` and compared by order, so equal-but-differently-ordered vectors
    /// are rejected as inconsistent). Disabling the pass keeps the export targets distinct.
    ///
    /// Disabling it on the stock path as well (not only for newyork) is intentional and not a
    /// size regression: the polkavm-linker performs its own function deduplication/outlining, so
    /// on the OpenZeppelin corpus dropping LLVM's `MergeFunctions` leaves real contracts slightly
    /// *smaller* rather than larger (only trivial interface blobs differ, by a few bytes). The
    /// newyork pipeline still merges where it helps, via the explicit `mergefunc` passes in its
    /// pass string.
    pub fn run_optimization_passes(
        &self,
        module: &inkwell::module::Module,
        passes: &str,
    ) -> Result<(), inkwell::support::LLVMString> {
        let pass_builder_options = inkwell::passes::PassBuilderOptions::create();
        pass_builder_options.set_verify_each(self.optimizer_settings.is_verify_each_enabled);
        pass_builder_options.set_debug_logging(self.optimizer_settings.is_debug_logging_enabled);
        pass_builder_options.set_loop_unrolling(
            self.optimizer_settings.level_middle_end_size == OptimizerSettingsSizeLevel::Zero,
        );
        pass_builder_options.set_merge_functions(false);

        module.run_passes(passes, &self.target_machine, pass_builder_options)
    }

    /// Returns the target triple.
    pub fn get_triple(&self) -> inkwell::targets::TargetTriple {
        self.target_machine.get_triple()
    }

    /// Returns the target data.
    pub fn get_target_data(&self) -> inkwell::targets::TargetData {
        self.target_machine.get_target_data()
    }
}
