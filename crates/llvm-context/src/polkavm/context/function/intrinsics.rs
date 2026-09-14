//! The LLVM intrinsic functions.

use std::num::NonZeroU32;

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
    /// Counts leading zeroes.
    pub count_leading_zeros: FunctionDeclaration<'ctx>,
    /// `(a + b) % n`, computed without truncating the sum.
    pub wide_add_mod: Option<FunctionDeclaration<'ctx>>,
    /// `(a * b) % n`, computed on the full 512-bit product.
    pub wide_mul_mod: Option<FunctionDeclaration<'ctx>>,
    /// Exponentiation, wrapping.
    pub wide_exponent: Option<FunctionDeclaration<'ctx>>,
    /// Sign extends its first operand from the byte its second names.
    pub wide_sign_extend: Option<FunctionDeclaration<'ctx>>,
}

impl<'ctx> Intrinsics<'ctx> {
    /// The corresponding intrinsic function name.
    pub const FUNCTION_TRAP: &'static str = "llvm.trap";

    /// The corresponding intrinsic function name.
    pub const FUNCTION_BYTE_SWAP_WORD: &'static str = "llvm.bswap.i256";

    /// The corresponding intrinsic function name.
    pub const FUNCTION_BYTE_SWAP_ETH_ADDRESS: &'static str = "llvm.bswap.i160";

    /// The corresponding intrinsic function name.
    pub const FUNCTION_COUNT_LEADING_ZEROS: &'static str = "llvm.ctlz.i256";

    /// The corresponding intrinsic function name.
    ///
    /// This and the three below come with the wide integer extension. They are looked up rather
    /// than assumed, because a build linked against an LLVM without the extension still has to
    /// compile: the EVM operations they stand for then go back to their `stdlib.ll` routines.
    /// The lookup happens only when [`crate::wide_integer_extension_enabled`] holds, since that
    /// is also what puts the feature into the target machine: an intrinsic declared without the
    /// feature would select nothing.
    pub const FUNCTION_WIDE_ADD_MOD: &'static str = "llvm.riscv.revive.addmod";

    /// The corresponding intrinsic function name.
    pub const FUNCTION_WIDE_MUL_MOD: &'static str = "llvm.riscv.revive.mulmod";

    /// The corresponding intrinsic function name.
    pub const FUNCTION_WIDE_EXPONENT: &'static str = "llvm.riscv.revive.exp";

    /// The corresponding intrinsic function name.
    pub const FUNCTION_WIDE_SIGN_EXTEND: &'static str = "llvm.riscv.revive.signextend";

    /// A shortcut constructor.
    pub fn new(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
    ) -> Self {
        let void_type = llvm.void_type();
        let word_type = llvm
            .custom_width_int_type(
                NonZeroU32::new(revive_common::BIT_LENGTH_WORD as u32).expect("const is non-zero"),
            )
            .expect("valid integer width");
        let address_type = llvm
            .custom_width_int_type(
                NonZeroU32::new(revive_common::BIT_LENGTH_ETH_ADDRESS as u32)
                    .expect("const is non-zero"),
            )
            .expect("valid integer width");

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
        let count_leading_zeros = Self::declare(
            llvm,
            module,
            Self::FUNCTION_COUNT_LEADING_ZEROS,
            word_type.fn_type(&[word_type.into(), llvm.bool_type().into()], false),
        );

        let modular_type = word_type.fn_type(
            &[word_type.into(), word_type.into(), word_type.into()],
            false,
        );
        let binary_type = word_type.fn_type(&[word_type.into(), word_type.into()], false);
        let (wide_add_mod, wide_mul_mod, wide_exponent, wide_sign_extend) =
            if crate::wide_integer_extension_enabled() {
                (
                    Self::try_declare(module, Self::FUNCTION_WIDE_ADD_MOD, modular_type),
                    Self::try_declare(module, Self::FUNCTION_WIDE_MUL_MOD, modular_type),
                    Self::try_declare(module, Self::FUNCTION_WIDE_EXPONENT, binary_type),
                    Self::try_declare(module, Self::FUNCTION_WIDE_SIGN_EXTEND, binary_type),
                )
            } else {
                (None, None, None, None)
            };

        Self {
            trap,
            byte_swap_word,
            byte_swap_eth_address,
            count_leading_zeros,
            wide_add_mod,
            wide_mul_mod,
            wide_exponent,
            wide_sign_extend,
        }
    }

    /// Whether this module compiles with the wide integer extension.
    ///
    /// True when the linked LLVM provides the extension and the target machine requests
    /// it: the intrinsics are only declared when both hold, so the presence of one of
    /// them answers for the whole arrangement.
    pub fn has_wide_integer_extension(&self) -> bool {
        self.wide_add_mod.is_some()
    }

    /// Declares the intrinsic if the linked LLVM has it, and returns nothing if it does not.
    ///
    /// Only the extension's own intrinsics go through this: they take no argument types to
    /// select on, so the declaration is by name alone.
    pub fn try_declare(
        module: &inkwell::module::Module<'ctx>,
        name: &str,
        r#type: inkwell::types::FunctionType<'ctx>,
    ) -> Option<FunctionDeclaration<'ctx>> {
        let intrinsic = inkwell::intrinsics::Intrinsic::find(name)?;
        let value = intrinsic.get_declaration(module, &[])?;
        Some(FunctionDeclaration::new(r#type, value))
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
        let word_type = llvm
            .custom_width_int_type(
                NonZeroU32::new(revive_common::BIT_LENGTH_WORD as u32).expect("const is non-zero"),
            )
            .expect("valid integer width");

        match name {
            _ if name == Self::FUNCTION_BYTE_SWAP_WORD => vec![word_type.as_basic_type_enum()],
            _ if name == Self::FUNCTION_BYTE_SWAP_ETH_ADDRESS => {
                vec![llvm
                    .custom_width_int_type(
                        NonZeroU32::new(revive_common::BIT_LENGTH_ETH_ADDRESS as u32)
                            .expect("const is non-zero"),
                    )
                    .expect("valid integer width")
                    .as_basic_type_enum()]
            }
            _ if name == Self::FUNCTION_COUNT_LEADING_ZEROS => {
                vec![word_type.as_basic_type_enum()]
            }
            _ => vec![],
        }
    }
}
