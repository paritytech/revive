//! The LLVM debug information.

use inkwell::debug_info::AsDIScope;
use num::Zero;

/// The LLVM debug information.
pub struct DebugInfo<'ctx> {
    /// The compile unit.
    compile_unit: inkwell::debug_info::DICompileUnit<'ctx>,
    /// The debug info builder.
    builder: inkwell::debug_info::DebugInfoBuilder<'ctx>,
}

impl<'ctx> DebugInfo<'ctx> {
    /// A shortcut constructor.
    pub fn new(module: &inkwell::module::Module<'ctx>) -> Self {
        let (builder, compile_unit) = module.create_debug_info_builder(
            true,
            inkwell::debug_info::DWARFSourceLanguage::C,
            module.get_name().to_string_lossy().as_ref(),
            "",
            "",
            false,
            "",
            0,
            "",
            inkwell::debug_info::DWARFEmissionKind::Full,
            0,
            false,
            false,
            "",
            "",
        );

        Self {
            compile_unit,
            builder,
        }
    }

    /// Creates a function info.
    pub fn create_function(
        &self,
        name: &str,
    ) -> anyhow::Result<inkwell::debug_info::DISubprogram<'ctx>> {
        let subroutine_type = self.builder.create_subroutine_type(
            self.compile_unit.get_file(),
            Some(self.create_type(revive_common::BIT_LENGTH_FIELD)?),
            &[],
            inkwell::debug_info::DIFlags::zero(),
        );

        let function = self.builder.create_function(
            self.compile_unit.get_file().as_debug_info_scope(),
            name,
            None,
            self.compile_unit.get_file(),
            42,
            subroutine_type,
            true,
            false,
            1,
            inkwell::debug_info::DIFlags::zero(),
            false,
        );

        self.builder.create_lexical_block(
            function.as_debug_info_scope(),
            self.compile_unit.get_file(),
            1,
            1,
        );

        Ok(function)
    }

    /// Creates a primitive type info.
    pub fn create_type(
        &self,
        bit_length: usize,
    ) -> anyhow::Result<inkwell::debug_info::DIType<'ctx>> {
        self.builder
            .create_basic_type(
                "U256",
                bit_length as u64,
                0,
                inkwell::debug_info::DIFlags::zero(),
            )
            .map(|basic_type| basic_type.as_type())
            .map_err(|error| anyhow::anyhow!("Debug info error: {}", error))
    }

    /// Finalizes the builder.
    pub fn finalize(&self) {
        self.builder.finalize();
    }
}
