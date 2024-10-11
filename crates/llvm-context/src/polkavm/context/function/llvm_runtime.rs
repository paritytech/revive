//! The LLVM runtime functions.

use inkwell::types::BasicType;

use crate::optimizer::Optimizer;
use crate::polkavm::context::address_space::AddressSpace;
use crate::polkavm::context::function::declaration::Declaration as FunctionDeclaration;
use crate::polkavm::context::function::Function;

/// The runtime functions, implemented on the LLVM side.
/// The functions are automatically linked to the LLVM implementations if the signatures match.
#[derive(Debug)]
pub struct LLVMRuntime<'ctx> {
    /// The corresponding LLVM runtime function.
    pub add_mod: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub mul_mod: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub exp: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub sign_extend: FunctionDeclaration<'ctx>,

    /// The corresponding LLVM runtime function.
    pub sha3: FunctionDeclaration<'ctx>,
}

impl<'ctx> LLVMRuntime<'ctx> {
    /// The corresponding runtime function name.
    pub const FUNCTION_ADDMOD: &'static str = "__addmod";

    /// The corresponding runtime function name.
    pub const FUNCTION_MULMOD: &'static str = "__mulmod";

    /// The corresponding runtime function name.
    pub const FUNCTION_EXP: &'static str = "__exp";

    /// The corresponding runtime function name.
    pub const FUNCTION_SIGNEXTEND: &'static str = "__signextend";

    /// The corresponding runtime function name.
    pub const FUNCTION_SHA3: &'static str = "__sha3";

    /// A shortcut constructor.
    pub fn new(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
        optimizer: &Optimizer,
    ) -> Self {
        let add_mod =
            Self::define(module, Self::FUNCTION_ADDMOD).expect("should be declared in stdlib");
        Function::set_default_attributes(llvm, add_mod, optimizer);
        Function::set_pure_function_attributes(llvm, add_mod);

        let mul_mod =
            Self::define(module, Self::FUNCTION_MULMOD).expect("should be declared in stdlib");
        Function::set_default_attributes(llvm, mul_mod, optimizer);
        Function::set_pure_function_attributes(llvm, mul_mod);

        let exp = Self::define(module, Self::FUNCTION_EXP).expect("should be declared in stdlib");
        Function::set_default_attributes(llvm, exp, optimizer);
        Function::set_pure_function_attributes(llvm, exp);

        let sign_extend =
            Self::define(module, Self::FUNCTION_SIGNEXTEND).expect("should be declared in stdlib");
        Function::set_default_attributes(llvm, sign_extend, optimizer);
        Function::set_pure_function_attributes(llvm, sign_extend);

        let sha3 = Self::declare(
            module,
            Self::FUNCTION_SHA3,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
                .fn_type(
                    vec![
                        llvm.ptr_type(AddressSpace::Heap.into())
                            .as_basic_type_enum()
                            .into(),
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
                            .as_basic_type_enum()
                            .into(),
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_BOOLEAN as u32)
                            .as_basic_type_enum()
                            .into(),
                    ]
                    .as_slice(),
                    false,
                ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, sha3, optimizer);
        Function::set_attributes(
            llvm,
            sha3,
            //vec![Attribute::ArgMemOnly, Attribute::ReadOnly],
            vec![],
            false,
        );

        Self {
            add_mod,
            mul_mod,
            exp,
            sign_extend,

            sha3,
        }
    }

    /// Declares an LLVM runtime function in the `module`,
    pub fn declare(
        module: &inkwell::module::Module<'ctx>,
        name: &str,
        r#type: inkwell::types::FunctionType<'ctx>,
        linkage: Option<inkwell::module::Linkage>,
    ) -> FunctionDeclaration<'ctx> {
        let value = module.add_function(name, r#type, linkage);
        FunctionDeclaration::new(r#type, value)
    }

    /// Create the function definition from an existing symbol.
    pub fn define(
        module: &inkwell::module::Module<'ctx>,
        name: &str,
    ) -> Option<FunctionDeclaration<'ctx>> {
        let value = module.get_function(name)?;
        value.set_linkage(inkwell::module::Linkage::External);
        FunctionDeclaration::new(value.get_type(), value).into()
    }
}
