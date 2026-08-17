//! The LLVM runtime functions.

use std::num::NonZeroU32;

use crate::optimizer::Optimizer;
use crate::polkavm::context::function::{
    declaration::Declaration as FunctionDeclaration, intrinsics::Intrinsics, Function,
};

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

    /// The corresponding intrinsic name. Operands are `(augend, addend, modulus)`.
    pub const INTRINSIC_ADDMOD: &'static str = "llvm.riscv.revive.addmod";

    /// The corresponding intrinsic name. Operands are `(multiplier, multiplicand, modulus)`.
    pub const INTRINSIC_MULMOD: &'static str = "llvm.riscv.revive.mulmod";

    /// The corresponding intrinsic name. Operands are `(base, exponent)`.
    pub const INTRINSIC_EXP: &'static str = "llvm.riscv.revive.exp";

    /// The corresponding intrinsic name. Operands are `(byte_index, value)`.
    pub const INTRINSIC_SIGNEXTEND: &'static str = "llvm.riscv.revive.signextend";

    /// A shortcut constructor.
    pub fn new(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
        optimizer: &Optimizer,
    ) -> Self {
        if optimizer.settings().wide_instructions {
            Self::new_intrinsics(llvm, module)
        } else {
            Self::new_stdlib(llvm, module, optimizer)
        }
    }

    /// Binds the `stdlib.ll` routines, which implement these operations in LLVM IR.
    fn new_stdlib(
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

        Self {
            add_mod,
            mul_mod,
            exp,
            sign_extend,
        }
    }

    /// Declares the custom intrinsics, each of which selects to a single instruction, and
    /// demotes the `stdlib.ll` routines they replace so that global DCE can drop the bodies.
    ///
    /// The declarations carry no attributes of their own: LLVM applies the intrinsic's own
    /// attribute set when the name resolves to an intrinsic ID, and `memory(none)` from that
    /// set is what lets equal calls be merged and hoisted, exactly as `stdlib.ll`'s `readnone`
    /// does on the other path.
    fn new_intrinsics(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
    ) -> Self {
        let word_type = llvm
            .custom_width_int_type(
                NonZeroU32::new(revive_common::BIT_LENGTH_WORD as u32).expect("const is non-zero"),
            )
            .expect("valid integer width");
        let binary_type = word_type.fn_type(&[word_type.into(), word_type.into()], false);
        let ternary_type = word_type.fn_type(
            &[word_type.into(), word_type.into(), word_type.into()],
            false,
        );

        // `stdlib.ll` gives these external linkage, which global DCE cannot remove. Demoting
        // them is what lets the bodies go once the intrinsics replace every call.
        for name in [
            Self::FUNCTION_ADDMOD,
            Self::FUNCTION_MULMOD,
            Self::FUNCTION_EXP,
            Self::FUNCTION_SIGNEXTEND,
        ] {
            module
                .get_function(name)
                .expect("should be declared in stdlib")
                .set_linkage(inkwell::module::Linkage::Private);
        }

        Self {
            add_mod: Intrinsics::declare(llvm, module, Self::INTRINSIC_ADDMOD, ternary_type),
            mul_mod: Intrinsics::declare(llvm, module, Self::INTRINSIC_MULMOD, ternary_type),
            exp: Intrinsics::declare(llvm, module, Self::INTRINSIC_EXP, binary_type),
            sign_extend: Intrinsics::declare(llvm, module, Self::INTRINSIC_SIGNEXTEND, binary_type),
        }
    }

    /// Create the function definition from an existing symbol.
    pub fn define(
        module: &inkwell::module::Module<'ctx>,
        name: &str,
    ) -> Option<FunctionDeclaration<'ctx>> {
        let value = module.get_function(name)?;
        value.set_linkage(inkwell::module::Linkage::Private);
        FunctionDeclaration::new(value.get_type(), value).into()
    }
}
