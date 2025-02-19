//! The LLVM pointer.

use inkwell::types::BasicType;

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::global::Global;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

pub mod heap;
pub mod storage;

/// The LLVM pointer.
#[derive(Debug, Clone, Copy)]
pub struct Pointer<'ctx> {
    /// The pointee type.
    pub r#type: inkwell::types::BasicTypeEnum<'ctx>,
    /// The address space.
    pub address_space: AddressSpace,
    /// The pointer value.
    pub value: inkwell::values::PointerValue<'ctx>,
}

impl<'ctx> Pointer<'ctx> {
    /// A shortcut constructor.
    pub fn new<T>(
        r#type: T,
        address_space: AddressSpace,
        value: inkwell::values::PointerValue<'ctx>,
    ) -> Self
    where
        T: BasicType<'ctx>,
    {
        Self {
            r#type: r#type.as_basic_type_enum(),
            address_space,
            value,
        }
    }

    /// Wraps a 256-bit primitive type pointer.
    pub fn new_stack_field<D>(
        context: &Context<'ctx, D>,
        value: inkwell::values::PointerValue<'ctx>,
    ) -> Self
    where
        D: Dependency + Clone,
    {
        Self {
            r#type: context.word_type().as_basic_type_enum(),
            address_space: AddressSpace::Stack,
            value,
        }
    }

    /// Creates a new pointer with the specified `offset`.
    pub fn new_with_offset<D, T>(
        context: &Context<'ctx, D>,
        address_space: AddressSpace,
        r#type: T,
        offset: inkwell::values::IntValue<'ctx>,
        name: &str,
    ) -> Self
    where
        D: Dependency + Clone,
        T: BasicType<'ctx>,
    {
        assert_ne!(
            address_space,
            AddressSpace::Stack,
            "Stack pointers cannot be addressed"
        );

        let offset = context.safe_truncate_int_to_xlen(offset).unwrap();
        let value = context
            .builder
            .build_int_to_ptr(offset, context.llvm().ptr_type(address_space.into()), name)
            .unwrap();
        Self::new(r#type, address_space, value)
    }

    /// Casts the pointer into another type.
    pub fn cast<T>(self, r#type: T) -> Self
    where
        T: BasicType<'ctx>,
    {
        Self {
            r#type: r#type.as_basic_type_enum(),
            address_space: self.address_space,
            value: self.value,
        }
    }

    /// Cast this pointer to a register sized integer value.
    pub fn to_int<D>(&self, context: &Context<'ctx, D>) -> inkwell::values::IntValue<'ctx>
    where
        D: Dependency + Clone,
    {
        context
            .builder()
            .build_ptr_to_int(self.value, context.xlen_type(), "ptr_to_xlen")
            .expect("we should be positioned")
    }

    pub fn address_space_cast<D>(
        self,
        context: &Context<'ctx, D>,
        address_space: AddressSpace,
        name: &str,
    ) -> anyhow::Result<Self>
    where
        D: Dependency + Clone,
    {
        let value = context.builder().build_address_space_cast(
            self.value,
            context.llvm().ptr_type(address_space.into()),
            name,
        )?;

        Ok(Self {
            address_space,
            value,
            ..self
        })
    }
}

impl<'ctx> From<Global<'ctx>> for Pointer<'ctx> {
    fn from(global: Global<'ctx>) -> Self {
        Self {
            r#type: global.r#type,
            address_space: AddressSpace::Stack,
            value: global.value.as_pointer_value(),
        }
    }
}
