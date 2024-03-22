//!
//! The LLVM IR generator function return entity.
//!

use crate::eravm::context::pointer::Pointer;

///
/// The LLVM IR generator function return entity.
///
#[derive(Debug, Clone, Copy)]
pub enum Return<'ctx> {
    /// The function does not return a value.
    None,
    /// The function returns a primitive value.
    Primitive {
        /// The primitive value pointer allocated at the function entry.
        pointer: Pointer<'ctx>,
    },
    /// The function returns a compound value.
    /// In this case, the return pointer is allocated on the stack by the callee.
    Compound {
        /// The structure pointer allocated at the function entry.
        pointer: Pointer<'ctx>,
        /// The function return type size.
        size: usize,
    },
}

impl<'ctx> Return<'ctx> {
    ///
    /// A shortcut constructor.
    ///
    pub fn none() -> Self {
        Self::None
    }

    ///
    /// A shortcut constructor.
    ///
    pub fn primitive(pointer: Pointer<'ctx>) -> Self {
        Self::Primitive { pointer }
    }

    ///
    /// A shortcut constructor.
    ///
    pub fn compound(pointer: Pointer<'ctx>, size: usize) -> Self {
        Self::Compound { pointer, size }
    }

    ///
    /// Returns the pointer to the function return value.
    ///
    pub fn return_pointer(&self) -> Option<Pointer<'ctx>> {
        match self {
            Return::None => None,
            Return::Primitive { pointer } => Some(pointer.to_owned()),
            Return::Compound { pointer, .. } => Some(pointer.to_owned()),
        }
    }

    ///
    /// Returns the return data size in bytes, based on the default stack alignment.
    ///
    pub fn return_data_size(&self) -> usize {
        revive_common::BYTE_LENGTH_FIELD
            * match self {
                Self::None => 0,
                Self::Primitive { .. } => 1,
                Self::Compound { size, .. } => *size,
            }
    }
}
