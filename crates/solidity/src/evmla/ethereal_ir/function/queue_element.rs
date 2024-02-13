//!
//! The Ethereal IR block queue element.
//!

use crate::evmla::ethereal_ir::function::block::element::stack::Stack;

///
/// The Ethereal IR block queue element.
///
#[derive(Debug, Clone)]
pub struct QueueElement {
    /// The block key.
    pub block_key: era_compiler_llvm_context::EraVMFunctionBlockKey,
    /// The block predecessor.
    pub predecessor: Option<(era_compiler_llvm_context::EraVMFunctionBlockKey, usize)>,
    /// The predecessor's last stack state.
    pub stack: Stack,
}

impl QueueElement {
    ///
    /// A shortcut constructor.
    ///
    pub fn new(
        block_key: era_compiler_llvm_context::EraVMFunctionBlockKey,
        predecessor: Option<(era_compiler_llvm_context::EraVMFunctionBlockKey, usize)>,
        stack: Stack,
    ) -> Self {
        Self {
            block_key,
            predecessor,
            stack,
        }
    }
}
