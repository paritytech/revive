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
    pub shl: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub shr: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub sar: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub byte: FunctionDeclaration<'ctx>,

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

    /// The corresponding LLVM runtime function.
    pub r#return: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub revert: FunctionDeclaration<'ctx>,
}

impl<'ctx> LLVMRuntime<'ctx> {
    /// The LLVM personality function name.
    pub const FUNCTION_PERSONALITY: &'static str = "__personality";

    /// The LLVM exception throwing function name.
    pub const FUNCTION_CXA_THROW: &'static str = "__cxa_throw";

    /// The corresponding runtime function name.
    pub const FUNCTION_SHL: &'static str = "__shl";

    /// The corresponding runtime function name.
    pub const FUNCTION_SHR: &'static str = "__shr";

    /// The corresponding runtime function name.
    pub const FUNCTION_SAR: &'static str = "__sar";

    /// The corresponding runtime function name.
    pub const FUNCTION_BYTE: &'static str = "__byte";

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

    /// The corresponding runtime function name.
    pub const FUNCTION_SYSTEM_REQUEST: &'static str = "__system_request";

    /// The corresponding runtime function name.
    pub const FUNCTION_FARCALL: &'static str = "__farcall";

    /// The corresponding runtime function name.
    pub const FUNCTION_STATICCALL: &'static str = "__staticcall";

    /// The corresponding runtime function name.
    pub const FUNCTION_DELEGATECALL: &'static str = "__delegatecall";

    /// The corresponding runtime function name.
    pub const FUNCTION_MIMICCALL: &'static str = "__mimiccall";

    /// The corresponding runtime function name.
    pub const FUNCTION_FARCALL_BYREF: &'static str = "__farcall_byref";

    /// The corresponding runtime function name.
    pub const FUNCTION_STATICCALL_BYREF: &'static str = "__staticcall_byref";

    /// The corresponding runtime function name.
    pub const FUNCTION_DELEGATECALL_BYREF: &'static str = "__delegatecall_byref";

    /// The corresponding runtime function name.
    pub const FUNCTION_MIMICCALL_BYREF: &'static str = "__mimiccall_byref";

    /// The corresponding runtime function name.
    pub const FUNCTION_RETURN: &'static str = "__return";

    /// The corresponding runtime function name.
    pub const FUNCTION_REVERT: &'static str = "__revert";

    /// A shortcut constructor.
    pub fn new(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
        optimizer: &Optimizer,
    ) -> Self {
        let shl = Self::declare(
            module,
            Self::FUNCTION_SHL,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
                            .as_basic_type_enum()
                            .into();
                        2
                    ]
                    .as_slice(),
                    false,
                ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, shl, optimizer);
        Function::set_pure_function_attributes(llvm, shl);

        let shr = Self::declare(
            module,
            Self::FUNCTION_SHR,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
                            .as_basic_type_enum()
                            .into();
                        2
                    ]
                    .as_slice(),
                    false,
                ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, shr, optimizer);
        Function::set_pure_function_attributes(llvm, shr);

        let sar = Self::declare(
            module,
            Self::FUNCTION_SAR,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
                            .as_basic_type_enum()
                            .into();
                        2
                    ]
                    .as_slice(),
                    false,
                ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, sar, optimizer);
        Function::set_pure_function_attributes(llvm, sar);

        let byte = Self::declare(
            module,
            Self::FUNCTION_BYTE,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
                            .as_basic_type_enum()
                            .into();
                        2
                    ]
                    .as_slice(),
                    false,
                ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, byte, optimizer);
        Function::set_pure_function_attributes(llvm, byte);

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

        let r#return = Self::declare(
            module,
            Self::FUNCTION_RETURN,
            llvm.void_type().fn_type(
                vec![
                    llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
                        .as_basic_type_enum()
                        .into();
                    3
                ]
                .as_slice(),
                false,
            ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, r#return, optimizer);
        let revert = Self::declare(
            module,
            Self::FUNCTION_REVERT,
            llvm.void_type().fn_type(
                vec![
                    llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
                        .as_basic_type_enum()
                        .into();
                    3
                ]
                .as_slice(),
                false,
            ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, revert, optimizer);

        Self {
            shl,
            shr,
            sar,
            byte,

            add_mod,
            mul_mod,
            exp,
            sign_extend,

            sha3,

            r#return,
            revert,
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
