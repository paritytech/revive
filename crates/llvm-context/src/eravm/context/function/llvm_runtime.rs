//!
//! The LLVM runtime functions.
//!

use inkwell::types::BasicType;

use crate::eravm::context::address_space::AddressSpace;
use crate::eravm::context::attribute::Attribute;
use crate::eravm::context::function::declaration::Declaration as FunctionDeclaration;
use crate::eravm::context::function::Function;
use crate::optimizer::Optimizer;

///
/// The runtime functions, implemented on the LLVM side.
///
/// The functions are automatically linked to the LLVM implementations if the signatures match.
///
#[derive(Debug)]
pub struct LLVMRuntime<'ctx> {
    /// The LLVM personality function, used for exception handling.
    pub personality: FunctionDeclaration<'ctx>,
    /// The LLVM exception throwing function.
    pub cxa_throw: FunctionDeclaration<'ctx>,

    /// The corresponding LLVM runtime function.
    pub div: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub sdiv: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub r#mod: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub smod: FunctionDeclaration<'ctx>,

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
    pub mstore8: FunctionDeclaration<'ctx>,

    /// The corresponding LLVM runtime function.
    pub sha3: FunctionDeclaration<'ctx>,

    /// The corresponding LLVM runtime function.
    pub system_request: FunctionDeclaration<'ctx>,

    /// The corresponding LLVM runtime function.
    //pub far_call: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub far_call_byref: FunctionDeclaration<'ctx>,

    /// The corresponding LLVM runtime function.
    pub static_call: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub static_call_byref: FunctionDeclaration<'ctx>,

    /// The corresponding LLVM runtime function.
    pub delegate_call: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub delegate_call_byref: FunctionDeclaration<'ctx>,

    /// The corresponding LLVM runtime function.
    pub mimic_call: FunctionDeclaration<'ctx>,
    /// The corresponding LLVM runtime function.
    pub mimic_call_byref: FunctionDeclaration<'ctx>,

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
    pub const FUNCTION_DIV: &'static str = "__div";

    /// The corresponding runtime function name.
    pub const FUNCTION_SDIV: &'static str = "__sdiv";

    /// The corresponding runtime function name.
    pub const FUNCTION_MOD: &'static str = "__mod";

    /// The corresponding runtime function name.
    pub const FUNCTION_SMOD: &'static str = "__smod";

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
    pub const FUNCTION_MSTORE8: &'static str = "__mstore8";

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

    ///
    /// A shortcut constructor.
    ///
    pub fn new(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
        optimizer: &Optimizer,
    ) -> Self {
        let personality = Self::declare(
            module,
            Self::FUNCTION_PERSONALITY,
            llvm.i32_type().fn_type(&[], false),
            None,
        );

        let cxa_throw = Self::declare(
            module,
            Self::FUNCTION_CXA_THROW,
            llvm.void_type().fn_type(
                vec![
                    llvm.i8_type()
                        .ptr_type(AddressSpace::Stack.into())
                        .as_basic_type_enum()
                        .into();
                    3
                ]
                .as_slice(),
                false,
            ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_cxa_throw_attributes(llvm, cxa_throw);

        let div = Self::declare(
            module,
            Self::FUNCTION_DIV,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                            .as_basic_type_enum()
                            .into();
                        2
                    ]
                    .as_slice(),
                    false,
                ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, div, optimizer);
        Function::set_pure_function_attributes(llvm, div);

        let r#mod = Self::declare(
            module,
            Self::FUNCTION_MOD,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                            .as_basic_type_enum()
                            .into();
                        2
                    ]
                    .as_slice(),
                    false,
                ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, r#mod, optimizer);
        Function::set_pure_function_attributes(llvm, r#mod);

        let sdiv = Self::declare(
            module,
            Self::FUNCTION_SDIV,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                            .as_basic_type_enum()
                            .into();
                        2
                    ]
                    .as_slice(),
                    false,
                ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, sdiv, optimizer);
        Function::set_pure_function_attributes(llvm, sdiv);

        let smod = Self::declare(
            module,
            Self::FUNCTION_SMOD,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                            .as_basic_type_enum()
                            .into();
                        2
                    ]
                    .as_slice(),
                    false,
                ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, smod, optimizer);
        Function::set_pure_function_attributes(llvm, smod);

        let shl = Self::declare(
            module,
            Self::FUNCTION_SHL,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
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
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
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
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
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
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
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

        let add_mod = Self::declare(
            module,
            Self::FUNCTION_ADDMOD,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                            .as_basic_type_enum()
                            .into();
                        3
                    ]
                    .as_slice(),
                    false,
                ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, add_mod, optimizer);
        Function::set_pure_function_attributes(llvm, add_mod);

        let mul_mod = Self::declare(
            module,
            Self::FUNCTION_MULMOD,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                            .as_basic_type_enum()
                            .into();
                        3
                    ]
                    .as_slice(),
                    false,
                ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, mul_mod, optimizer);
        Function::set_pure_function_attributes(llvm, mul_mod);

        let exp = Self::declare(
            module,
            Self::FUNCTION_EXP,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                            .as_basic_type_enum()
                            .into();
                        2
                    ]
                    .as_slice(),
                    false,
                ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, exp, optimizer);
        Function::set_pure_function_attributes(llvm, exp);

        let sign_extend = FunctionDeclaration::new(
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                            .as_basic_type_enum()
                            .into();
                        2
                    ]
                    .as_slice(),
                    false,
                ),
            module
                .get_function(Self::FUNCTION_SIGNEXTEND)
                .expect("should be declared in stdlib"),
        );
        Function::set_default_attributes(llvm, sign_extend, optimizer);
        Function::set_pure_function_attributes(llvm, sign_extend);

        let mstore8 = Self::declare(
            module,
            Self::FUNCTION_MSTORE8,
            llvm.void_type().fn_type(
                vec![
                    llvm.custom_width_int_type(revive_common::BIT_LENGTH_BYTE as u32)
                        .ptr_type(AddressSpace::Heap.into())
                        .as_basic_type_enum()
                        .into(),
                    llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                        .as_basic_type_enum()
                        .into(),
                ]
                .as_slice(),
                false,
            ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, mstore8, optimizer);
        Function::set_attributes(
            llvm,
            mstore8,
            vec![
                Attribute::MustProgress,
                Attribute::NoUnwind,
                Attribute::WillReturn,
            ],
            false,
        );

        let sha3 = Self::declare(
            module,
            Self::FUNCTION_SHA3,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_BYTE as u32)
                            .ptr_type(AddressSpace::Heap.into())
                            .as_basic_type_enum()
                            .into(),
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
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

        let system_request = Self::declare(
            module,
            Self::FUNCTION_SYSTEM_REQUEST,
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .fn_type(
                    vec![
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                            .as_basic_type_enum()
                            .into(),
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                            .as_basic_type_enum()
                            .into(),
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                            .as_basic_type_enum()
                            .into(),
                        llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                            .ptr_type(AddressSpace::Stack.into())
                            .as_basic_type_enum()
                            .into(),
                    ]
                    .as_slice(),
                    false,
                ),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, system_request, optimizer);
        Function::set_attributes(
            llvm,
            system_request,
            //vec![Attribute::ArgMemOnly, Attribute::ReadOnly],
            vec![],
            false,
        );

        let external_call_arguments: Vec<inkwell::types::BasicMetadataTypeEnum> = vec![
                llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                    .as_basic_type_enum()
                    .into();
                crate::eravm::context::function::runtime::entry::Entry::MANDATORY_ARGUMENTS_COUNT
                    + crate::eravm::EXTRA_ABI_DATA_SIZE
            ];
        let mut mimic_call_arguments = external_call_arguments.clone();
        mimic_call_arguments.push(
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .as_basic_type_enum()
                .into(),
        );

        let mut external_call_arguments_by_ref: Vec<inkwell::types::BasicMetadataTypeEnum> = vec![
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_BYTE as u32)
                .ptr_type(AddressSpace::Generic.into())
                .as_basic_type_enum()
                .into(),
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .as_basic_type_enum()
                .into(),
        ];
        external_call_arguments_by_ref.extend::<Vec<inkwell::types::BasicMetadataTypeEnum>>(vec![
            llvm.custom_width_int_type(
                revive_common::BIT_LENGTH_FIELD as u32
            )
            .as_basic_type_enum()
            .into();
            crate::eravm::EXTRA_ABI_DATA_SIZE
        ]);
        let mut mimic_call_arguments_by_ref = external_call_arguments_by_ref.clone();
        mimic_call_arguments_by_ref.push(
            llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
                .as_basic_type_enum()
                .into(),
        );

        let external_call_result_type = llvm
            .struct_type(
                &[
                    llvm.custom_width_int_type(revive_common::BIT_LENGTH_BYTE as u32)
                        .ptr_type(AddressSpace::Generic.into())
                        .as_basic_type_enum(),
                    llvm.bool_type().as_basic_type_enum(),
                ],
                false,
            )
            .as_basic_type_enum();

        //let far_call = Self::declare(
        //    module,
        //    Self::FUNCTION_FARCALL,
        //    external_call_result_type.fn_type(external_call_arguments.as_slice(), false),
        //    Some(inkwell::module::Linkage::External),
        //);
        //Function::set_default_attributes(llvm, far_call, optimizer);
        let static_call = Self::declare(
            module,
            Self::FUNCTION_STATICCALL,
            external_call_result_type.fn_type(external_call_arguments.as_slice(), false),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, static_call, optimizer);
        let delegate_call = Self::declare(
            module,
            Self::FUNCTION_DELEGATECALL,
            external_call_result_type.fn_type(external_call_arguments.as_slice(), false),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, delegate_call, optimizer);
        let mimic_call = Self::declare(
            module,
            Self::FUNCTION_MIMICCALL,
            external_call_result_type.fn_type(mimic_call_arguments.as_slice(), false),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, mimic_call, optimizer);

        let far_call_byref = Self::declare(
            module,
            Self::FUNCTION_FARCALL_BYREF,
            external_call_result_type.fn_type(external_call_arguments_by_ref.as_slice(), false),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, far_call_byref, optimizer);
        let static_call_byref = Self::declare(
            module,
            Self::FUNCTION_STATICCALL_BYREF,
            external_call_result_type.fn_type(external_call_arguments_by_ref.as_slice(), false),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, static_call_byref, optimizer);
        let delegate_call_byref = Self::declare(
            module,
            Self::FUNCTION_DELEGATECALL_BYREF,
            external_call_result_type.fn_type(external_call_arguments_by_ref.as_slice(), false),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, delegate_call_byref, optimizer);
        let mimic_call_byref = Self::declare(
            module,
            Self::FUNCTION_MIMICCALL_BYREF,
            external_call_result_type.fn_type(mimic_call_arguments_by_ref.as_slice(), false),
            Some(inkwell::module::Linkage::External),
        );
        Function::set_default_attributes(llvm, mimic_call_byref, optimizer);

        let r#return = Self::declare(
            module,
            Self::FUNCTION_RETURN,
            llvm.void_type().fn_type(
                vec![
                    llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
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
                    llvm.custom_width_int_type(revive_common::BIT_LENGTH_FIELD as u32)
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
            personality,
            cxa_throw,

            div,
            sdiv,
            r#mod,
            smod,

            shl,
            shr,
            sar,
            byte,

            add_mod,
            mul_mod,
            exp,
            sign_extend,

            mstore8,

            sha3,

            system_request,

            //far_call,
            static_call,
            delegate_call,
            mimic_call,

            far_call_byref,
            static_call_byref,
            delegate_call_byref,
            mimic_call_byref,

            r#return,
            revert,
        }
    }

    ///
    /// Declares an LLVM runtime function in the `module`,
    ///
    pub fn declare(
        module: &inkwell::module::Module<'ctx>,
        name: &str,
        r#type: inkwell::types::FunctionType<'ctx>,
        linkage: Option<inkwell::module::Linkage>,
    ) -> FunctionDeclaration<'ctx> {
        let value = module.add_function(name, r#type, linkage);
        FunctionDeclaration::new(r#type, value)
    }

    ///
    /// Modifies the external call function with `is_byref` and `is_system` modifiers.
    ///
    pub fn modify(
        &self,
        function: FunctionDeclaration<'ctx>,
        is_byref: bool,
    ) -> anyhow::Result<FunctionDeclaration<'ctx>> {
        let modified = if
        /*function == self.far_call {
            match is_byref {
                false => self.far_call,
                true => self.far_call_byref,
            }
        } else if */
        function == self.static_call {
            match is_byref {
                false => self.static_call,
                true => self.static_call_byref,
            }
        } else if function == self.delegate_call {
            match is_byref {
                false => self.delegate_call,
                true => self.delegate_call_byref,
            }
        } else if function == self.mimic_call {
            match is_byref {
                false => self.mimic_call,
                true => self.mimic_call_byref,
            }
        } else {
            anyhow::bail!(
                "Cannot modify an external call function `{}`",
                function.value.get_name().to_string_lossy()
            );
        };

        Ok(modified)
    }
}
