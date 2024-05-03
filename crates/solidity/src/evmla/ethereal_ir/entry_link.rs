//! The Ethereal IR entry function link.

use inkwell::values::BasicValue;

use crate::evmla::ethereal_ir::EtherealIR;

/// The Ethereal IR entry function link.
/// The link represents branching between the deploy and runtime code.
#[derive(Debug, Clone)]
pub struct EntryLink {
    /// The code part type.
    pub code_type: revive_llvm_context::PolkaVMCodeType,
}

impl EntryLink {
    /// A shortcut constructor.
    pub fn new(code_type: revive_llvm_context::PolkaVMCodeType) -> Self {
        Self { code_type }
    }
}

impl<D> revive_llvm_context::PolkaVMWriteLLVM<D> for EntryLink
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    fn into_llvm(self, context: &mut revive_llvm_context::PolkaVMContext<D>) -> anyhow::Result<()> {
        let target = context
            .get_function(EtherealIR::DEFAULT_ENTRY_FUNCTION_NAME)
            .expect("Always exists")
            .borrow()
            .declaration();
        let is_deploy_code = match self.code_type {
            revive_llvm_context::PolkaVMCodeType::Deploy => context
                .integer_type(revive_common::BIT_LENGTH_BOOLEAN)
                .const_int(1, false),
            revive_llvm_context::PolkaVMCodeType::Runtime => context
                .integer_type(revive_common::BIT_LENGTH_BOOLEAN)
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
