//!
//! The LLVM intrinsic functions.
//!

use inkwell::types::BasicType;

use crate::eravm::context::address_space::AddressSpace;
use crate::eravm::context::function::declaration::Declaration as FunctionDeclaration;

///
/// The LLVM intrinsic functions, implemented in the LLVM back-end.
///
/// Most of them are translated directly into bytecode instructions.
///
#[derive(Debug)]
pub struct Intrinsics<'ctx> {
    /// The trap.
    pub trap: FunctionDeclaration<'ctx>,
    /// The memory copy within the heap.
    pub memory_copy: FunctionDeclaration<'ctx>,
    /// The memory copy from a generic page.
    pub memory_copy_from_generic: FunctionDeclaration<'ctx>,
}

impl<'ctx> Intrinsics<'ctx> {
    /// The corresponding intrinsic function name.
    pub const FUNCTION_TRAP: &'static str = "llvm.trap";

    /// The corresponding intrinsic function name.
    pub const FUNCTION_MEMORY_COPY: &'static str = "llvm.memcpy.p1.p1.i256";

    /// The corresponding intrinsic function name.
    pub const FUNCTION_MEMORY_COPY_FROM_GENERIC: &'static str = "llvm.memcpy.p3.p1.i256";

    ///
    /// A shortcut constructor.
    ///
    pub fn new(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
    ) -> Self {
        let void_type = llvm.void_type();
        let bool_type = llvm.bool_type();
        let byte_type = llvm.custom_width_int_type(era_compiler_common::BIT_LENGTH_BYTE as u32);
        let field_type = llvm.custom_width_int_type(era_compiler_common::BIT_LENGTH_FIELD as u32);
        let _stack_field_pointer_type = field_type.ptr_type(AddressSpace::Stack.into());
        let heap_field_pointer_type = byte_type.ptr_type(AddressSpace::Heap.into());
        let generic_byte_pointer_type = byte_type.ptr_type(AddressSpace::Generic.into());

        let trap = Self::declare(
            llvm,
            module,
            Self::FUNCTION_TRAP,
            void_type.fn_type(&[], false),
        );
        let memory_copy = Self::declare(
            llvm,
            module,
            Self::FUNCTION_MEMORY_COPY,
            void_type.fn_type(
                &[
                    heap_field_pointer_type.as_basic_type_enum().into(),
                    heap_field_pointer_type.as_basic_type_enum().into(),
                    field_type.as_basic_type_enum().into(),
                    bool_type.as_basic_type_enum().into(),
                ],
                false,
            ),
        );
        let memory_copy_from_generic = Self::declare(
            llvm,
            module,
            Self::FUNCTION_MEMORY_COPY_FROM_GENERIC,
            void_type.fn_type(
                &[
                    heap_field_pointer_type.as_basic_type_enum().into(),
                    generic_byte_pointer_type.as_basic_type_enum().into(),
                    field_type.as_basic_type_enum().into(),
                    bool_type.as_basic_type_enum().into(),
                ],
                false,
            ),
        );

        Self {
            trap,
            memory_copy,
            memory_copy_from_generic,
        }
    }

    ///
    /// Finds the specified LLVM intrinsic function in the target and returns its declaration.
    ///
    pub fn declare(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
        name: &str,
        r#type: inkwell::types::FunctionType<'ctx>,
    ) -> FunctionDeclaration<'ctx> {
        let intrinsic = inkwell::intrinsics::Intrinsic::find(name)
            .unwrap_or_else(|| panic!("Intrinsic function `{name}` does not exist"));
        let argument_types = Self::argument_types(llvm, name);
        let value = intrinsic
            .get_declaration(module, argument_types.as_slice())
            .unwrap_or_else(|| panic!("Intrinsic function `{name}` declaration error"));
        FunctionDeclaration::new(r#type, value)
    }

    ///
    /// Returns the LLVM types for selecting via the signature.
    ///
    pub fn argument_types(
        llvm: &'ctx inkwell::context::Context,
        name: &str,
    ) -> Vec<inkwell::types::BasicTypeEnum<'ctx>> {
        let field_type = llvm.custom_width_int_type(era_compiler_common::BIT_LENGTH_FIELD as u32);

        match name {
            name if name == Self::FUNCTION_MEMORY_COPY => vec![
                field_type
                    .ptr_type(AddressSpace::Heap.into())
                    .as_basic_type_enum(),
                field_type
                    .ptr_type(AddressSpace::Heap.into())
                    .as_basic_type_enum(),
                field_type.as_basic_type_enum(),
            ],
            name if name == Self::FUNCTION_MEMORY_COPY_FROM_GENERIC => vec![
                field_type
                    .ptr_type(AddressSpace::Heap.into())
                    .as_basic_type_enum(),
                field_type
                    .ptr_type(AddressSpace::Generic.into())
                    .as_basic_type_enum(),
                field_type.as_basic_type_enum(),
            ],
            _ => vec![],
        }
    }
}
