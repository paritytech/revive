//! The LLVM intrinsic functions.

use inkwell::types::BasicType;

use crate::polkavm::context::function::declaration::Declaration as FunctionDeclaration;

/// The LLVM intrinsic functions, implemented in the LLVM back-end.
/// Most of them are translated directly into bytecode instructions.
#[derive(Debug)]
pub struct Intrinsics<'ctx> {
    /// The trap.
    pub trap: FunctionDeclaration<'ctx>,
    /// Performs endianness swaps on i256 values
    pub byte_swap_word: FunctionDeclaration<'ctx>,
    /// Performs endianness swaps on i160 values
    pub byte_swap_eth_address: FunctionDeclaration<'ctx>,
}

impl<'ctx> Intrinsics<'ctx> {
    /// The corresponding intrinsic function name.
    pub const FUNCTION_TRAP: &'static str = "llvm.trap";

    /// The corresponding intrinsic function name.
    pub const FUNCTION_BYTE_SWAP_WORD: &'static str = "llvm.bswap.i256";

    /// The corresponding intrinsic function name.
    pub const FUNCTION_BYTE_SWAP_ETH_ADDRESS: &'static str = "llvm.bswap.i160";

    /// A shortcut constructor.
    pub fn new(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
    ) -> Self {
        let void_type = llvm.void_type();
        let word_type = llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32);
        let address_type = llvm.custom_width_int_type(revive_common::BIT_LENGTH_ETH_ADDRESS as u32);

        let trap = Self::declare(
            llvm,
            module,
            Self::FUNCTION_TRAP,
            void_type.fn_type(&[], false),
        );
        let byte_swap_word = Self::declare(
            llvm,
            module,
            Self::FUNCTION_BYTE_SWAP_WORD,
            word_type.fn_type(&[word_type.as_basic_type_enum().into()], false),
        );
        let byte_swap_eth_address = Self::declare(
            llvm,
            module,
            Self::FUNCTION_BYTE_SWAP_ETH_ADDRESS,
            address_type.fn_type(&[address_type.as_basic_type_enum().into()], false),
        );

        Self {
            trap,
            byte_swap_word,
            byte_swap_eth_address,
        }
    }

    /// Finds the specified LLVM intrinsic function in the target and returns its declaration.
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

    /// Returns the LLVM types for selecting via the signature.
    pub fn argument_types(
        llvm: &'ctx inkwell::context::Context,
        name: &str,
    ) -> Vec<inkwell::types::BasicTypeEnum<'ctx>> {
        let word_type = llvm.custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32);

        match name {
            name if name == Self::FUNCTION_BYTE_SWAP_WORD => vec![word_type.as_basic_type_enum()],
            name if name == Self::FUNCTION_BYTE_SWAP_ETH_ADDRESS => {
                vec![llvm
                    .custom_width_int_type(revive_common::BIT_LENGTH_ETH_ADDRESS as u32)
                    .as_basic_type_enum()]
            }
            _ => vec![],
        }
    }
}
