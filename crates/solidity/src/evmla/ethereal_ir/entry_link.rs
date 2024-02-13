//!
//! The Ethereal IR entry function link.
//!

use inkwell::values::BasicValue;

use crate::evmla::ethereal_ir::EtherealIR;

///
/// The Ethereal IR entry function link.
///
/// The link represents branching between the deploy and runtime code.
///
#[derive(Debug, Clone)]
pub struct EntryLink {
    /// The code part type.
    pub code_type: era_compiler_llvm_context::EraVMCodeType,
}

impl EntryLink {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(code_type: era_compiler_llvm_context::EraVMCodeType) -> Self {
        Self { code_type }
    }
}

impl<D> era_compiler_llvm_context::EraVMWriteLLVM<D> for EntryLink
where
    D: era_compiler_llvm_context::EraVMDependency + Clone,
{
    fn into_llvm(
        self,
        context: &mut era_compiler_llvm_context::EraVMContext<D>,
    ) -> anyhow::Result<()> {
        let target = context
            .get_function(EtherealIR::DEFAULT_ENTRY_FUNCTION_NAME)
            .expect("Always exists")
            .borrow()
            .declaration();
        let is_deploy_code = match self.code_type {
            era_compiler_llvm_context::EraVMCodeType::Deploy => context
                .integer_type(era_compiler_common::BIT_LENGTH_BOOLEAN)
                .const_int(1, false),
            era_compiler_llvm_context::EraVMCodeType::Runtime => context
                .integer_type(era_compiler_common::BIT_LENGTH_BOOLEAN)
                .const_int(0, false),
        };
        context.build_invoke(
            target,
            &[is_deploy_code.as_basic_value_enum()],
            format!("call_link_{}", EtherealIR::DEFAULT_ENTRY_FUNCTION_NAME).as_str(),
        );

        Ok(())
    }
}
