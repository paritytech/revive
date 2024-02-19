//!
//! The LLVM global value.
//!

use inkwell::types::BasicType;
use inkwell::values::BasicValue;

use crate::eravm::context::address_space::AddressSpace;
use crate::eravm::context::Context;
use crate::EraVMDependency;

///
/// The LLVM global value.
///
#[derive(Debug, Clone, Copy)]
pub struct Global<'ctx> {
    /// The global type.
    pub r#type: inkwell::types::BasicTypeEnum<'ctx>,
    /// The global value.
    pub value: inkwell::values::GlobalValue<'ctx>,
}

impl<'ctx> Global<'ctx> {
    ///
    /// A shortcut constructor.
    ///
    pub fn new<D, T, V>(
        context: &mut Context<'ctx, D>,
        r#type: T,
        address_space: AddressSpace,
        initializer: V,
        name: &str,
    ) -> Self
    where
        D: EraVMDependency + Clone,
        T: BasicType<'ctx>,
        V: BasicValue<'ctx>,
    {
        let r#type = r#type.as_basic_type_enum();

        let value = context
            .module()
            .add_global(r#type, Some(address_space.into()), name);
        let global = Self { r#type, value };

        global.value.set_linkage(inkwell::module::Linkage::Private);
        global
            .value
            .set_visibility(inkwell::GlobalVisibility::Default);
        global.value.set_externally_initialized(false);
        if let AddressSpace::Code = address_space {
            global.value.set_constant(true);
        }
        if !r#type.is_pointer_type() {
            global.value.set_initializer(&initializer);
        } else {
            global.value.set_initializer(&r#type.const_zero());
            context.build_store(global.into(), initializer).unwrap();
        }

        global
    }
}
