//! The LLVM global value.

use inkwell::types::BasicType;
use inkwell::values::BasicValue;

use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::Context;

/// The LLVM global value.
#[derive(Debug, Clone, Copy)]
pub struct Global<'ctx> {
    /// The global type.
    pub r#type: inkwell::types::BasicTypeEnum<'ctx>,
    /// The global value.
    pub value: inkwell::values::GlobalValue<'ctx>,
}

impl<'ctx> Global<'ctx> {
    /// A shortcut constructor.
    pub fn new<T, V>(
        context: &mut Context<'ctx>,
        r#type: T,
        address_space: AddressSpace,
        initializer: V,
        name: &str,
    ) -> Self
    where
        T: BasicType<'ctx>,
        V: BasicValue<'ctx>,
    {
        let r#type = r#type.as_basic_type_enum();

        let value = context
            .module()
            .add_global(r#type, Some(address_space.into()), name);
        let global = Self { r#type, value };

        global.value.set_linkage(inkwell::module::Linkage::External);
        global
            .value
            .set_visibility(inkwell::GlobalVisibility::Default);
        global.value.set_externally_initialized(false);
        if !r#type.is_pointer_type() {
            global.value.set_initializer(&initializer);
        } else {
            global.value.set_initializer(&r#type.const_zero());
            context.build_store(global.into(), initializer).unwrap();
        }

        global
    }

    /// Construct an external global.
    pub fn declare<T>(
        context: &mut Context<'ctx>,
        r#type: T,
        address_space: AddressSpace,
        name: &str,
    ) -> Self
    where
        T: BasicType<'ctx>,
    {
        let r#type = r#type.as_basic_type_enum();

        let value = context
            .module()
            .add_global(r#type, Some(address_space.into()), name);
        let global = Self { r#type, value };

        global.value.set_linkage(inkwell::module::Linkage::External);
        global
            .value
            .set_visibility(inkwell::GlobalVisibility::Default);
        global.value.set_externally_initialized(true);

        global
    }
}
