//! The revive compiler runtime functions.

use inkwell::values::BasicValue;

use crate::polkavm::context::attribute::MemoryEffect;
use crate::polkavm::context::function::Attribute;
use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::WriteLLVM;

/// Pointers are represented as opaque 256 bit integer values in EVM.
/// In practice, they should never exceed a register sized bit value.
/// However, we still protect against this possibility here: Heap index
/// offsets are generally untrusted and potentially represent valid
/// (but wrong) pointers when truncated.
pub struct WordToPointer;

impl RuntimeFunction for WordToPointer {
    const NAME: &'static str = "__revive_int_truncate";

    const ATTRIBUTES: &'static [Attribute] = &[
        Attribute::WillReturn,
        Attribute::NoFree,
        Attribute::AlwaysInline,
    ];

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context
            .xlen_type()
            .fn_type(&[context.word_type().into()], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let value = Self::paramater(context, 0).into_int_value();
        let truncated =
            context
                .builder()
                .build_int_truncate(value, context.xlen_type(), "offset_truncated")?;
        let extended = context.builder().build_int_z_extend(
            truncated,
            context.word_type(),
            "offset_extended",
        )?;
        let is_overflow = context.builder().build_int_compare(
            inkwell::IntPredicate::NE,
            value,
            extended,
            "compare_truncated_extended",
        )?;

        let block_continue = context.append_basic_block("offset_pointer_ok");
        let block_invalid = context.append_basic_block("offset_pointer_overflow");
        context.build_conditional_branch(is_overflow, block_invalid, block_continue)?;

        context.set_basic_block(block_invalid);
        context.build_runtime_call(revive_runtime_api::polkavm_imports::INVALID, &[]);
        context.build_unreachable();

        context.set_basic_block(block_continue);
        Ok(Some(truncated.as_basic_value_enum()))
    }
}

impl WriteLLVM for WordToPointer {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// The revive runtime exit function.
pub struct Exit;

impl RuntimeFunction for Exit {
    const NAME: &'static str = "__revive_exit";

    const ATTRIBUTES: &'static [Attribute] = &[
        Attribute::NoReturn,
        Attribute::NoFree,
        Attribute::AlwaysInline,
    ];

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.void_type().fn_type(
            &[
                context.xlen_type().into(),
                context.word_type().into(),
                context.word_type().into(),
            ],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let flags = Self::paramater(context, 0).into_int_value();
        let offset = Self::paramater(context, 1).into_int_value();
        let length = Self::paramater(context, 2).into_int_value();

        let offset_truncated = context.safe_truncate_int_to_xlen(offset)?;
        let length_truncated = context.safe_truncate_int_to_xlen(length)?;
        let heap_pointer = context.build_heap_gep(offset_truncated, length_truncated)?;
        let offset_pointer = context.builder().build_ptr_to_int(
            heap_pointer.value,
            context.xlen_type(),
            "return_data_ptr_to_int",
        )?;

        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::RETURN,
            &[flags.into(), offset_pointer.into(), length_truncated.into()],
        );
        context.build_unreachable();

        Ok(None)
    }
}

impl WriteLLVM for Exit {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Outlined `callvalue()` function that reads the value transferred with the call.
///
/// The `value_transferred` runtime API writes a 256-bit value to a buffer.
/// Each inline call site generates: alloca i256 + store 0 + ptrtoint + call + load.
/// By outlining into a shared function, all call sites reduce to a single `call`.
/// Contracts with many non-payable checks (OpenZeppelin ERC20: 37 sites) save
/// significant code size through this deduplication.
pub struct CallValue;

impl RuntimeFunction for CallValue {
    const NAME: &'static str = "__revive_callvalue";

    const ATTRIBUTES: &'static [Attribute] = &[
        Attribute::NoFree,
        Attribute::NoRecurse,
        Attribute::WillReturn,
    ];

    /// The call value is constant for the duration of a contract invocation,
    /// so this helper is effectively pure: LLVM can DCE redundant calls and
    /// CSE duplicate calls.
    const MEMORY_EFFECT: MemoryEffect = MemoryEffect::None;

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.word_type().fn_type(&[], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let output_pointer =
            context.build_alloca_at_entry(context.value_type(), "value_transferred");
        context.build_store(output_pointer, context.word_const(0))?;
        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::VALUE_TRANSFERRED,
            &[output_pointer.to_int(context).into()],
        );
        let value = context.build_load(output_pointer, "value_transferred")?;
        Ok(Some(value))
    }
}

impl WriteLLVM for CallValue {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Outlined `callvalue() != 0` function that returns a boolean flag.
///
/// In ERC20 and similar contracts, callvalue is ONLY used for non-payable
/// checks: `if (callvalue()) { revert(0, 0) }`. Each such check compares
/// a 256-bit value against zero, generating 4 limb comparisons in PVM.
/// By outlining the check into a function returning `i1`, each call site
/// saves ~15 bytes (4 comparisons + OR vs. a single branch on i1).
/// OpenZeppelin ERC20 has ~20 such checks, saving ~300 bytes.
pub struct CallValueNonzero;

impl RuntimeFunction for CallValueNonzero {
    const NAME: &'static str = "__revive_callvalue_nonzero";

    const ATTRIBUTES: &'static [Attribute] = &[
        Attribute::NoFree,
        Attribute::NoRecurse,
        Attribute::WillReturn,
    ];

    /// Same `memory(none)` rationale as [`CallValue`].
    const MEMORY_EFFECT: MemoryEffect = MemoryEffect::None;

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.llvm().bool_type().fn_type(&[], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let output_pointer =
            context.build_alloca_at_entry(context.value_type(), "value_transferred");
        context.build_store(output_pointer, context.word_const(0))?;
        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::VALUE_TRANSFERRED,
            &[output_pointer.to_int(context).into()],
        );
        let value = context
            .build_load(output_pointer, "value_transferred")?
            .into_int_value();
        let zero = context.word_type().const_zero();
        let is_nonzero = context.builder().build_int_compare(
            inkwell::IntPredicate::NE,
            value,
            zero,
            "callvalue_nonzero",
        )?;
        Ok(Some(is_nonzero.into()))
    }
}

impl WriteLLVM for CallValueNonzero {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Outlined `calldataload(offset)` function that reads 32 bytes from call data.
///
/// The `call_data_load` runtime API writes a 256-bit value to a buffer.
/// Each inline call site generates: alloca i256 + ptrtoint + call + load.
/// By outlining into a shared function, all call sites reduce to `clip_to_xlen + call`.
/// Contracts with many ABI parameters (OpenZeppelin ERC20: 51 sites) save
/// significant code size through this deduplication.
pub struct CallDataLoad;

impl RuntimeFunction for CallDataLoad {
    const NAME: &'static str = "__revive_calldataload";

    /// Calldata is immutable for the duration of execution and only
    /// reachable through the host syscall. The helper's alloca write is
    /// invisible to callers, so the externally observable effect is a read
    /// of pallet-revive runtime state — letting LLVM GVN fold repeated
    /// `__revive_calldataload(offset)` calls even across heap mstores.
    const MEMORY_EFFECT: MemoryEffect = MemoryEffect::ReadInaccessible;

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context
            .word_type()
            .fn_type(&[context.xlen_type().into()], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let offset = Self::paramater(context, 0).into_int_value();
        let output_pointer = context.build_alloca_at_entry(context.word_type(), "call_data_output");
        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::CALL_DATA_LOAD,
            &[output_pointer.to_int(context).into(), offset.into()],
        );
        let value = context.build_load(output_pointer, "call_data_load_value")?;
        Ok(Some(value))
    }
}

impl WriteLLVM for CallDataLoad {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Outlined `caller()` function that reads the calling account address.
///
/// The `caller` runtime API writes a 20-byte address to a buffer.
/// Each inline call site generates: get_global + ptrtoint + call + load + bswap + zext.
/// By outlining into a shared function, all call sites reduce to a single `call`.
/// Contracts with many msg.sender checks (OpenZeppelin ERC20: 10+ sites) save
/// significant code size through this deduplication.
pub struct Caller;

impl RuntimeFunction for Caller {
    const NAME: &'static str = "__revive_caller";

    /// The caller is immutable within an execution and the syscall reads it
    /// from pallet-revive runtime state. The body writes the result into a
    /// function-local alloca (not the shared `@GLOBAL_ADDRESS_SPILL_BUFFER`,
    /// which `origin()` and `build_address` also touch — see the
    /// `caller_origin_aliasing` regression test), so from the caller's view
    /// only pallet-revive runtime state is read and `ReadInaccessible` is
    /// sound. This lets GVN deduplicate repeated `__revive_caller()` calls
    /// across heap mstores.
    const MEMORY_EFFECT: MemoryEffect = MemoryEffect::ReadInaccessible;

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.word_type().fn_type(&[], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let address_type = context.integer_type(revive_common::BIT_LENGTH_ETH_ADDRESS);
        let output_pointer = context.build_alloca_at_entry(address_type, "caller_output");
        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::CALLER,
            &[output_pointer.to_int(context).into()],
        );
        let value = context.build_load_address(output_pointer)?;
        Ok(Some(value))
    }
}

impl WriteLLVM for Caller {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Outlined `revert(0, 0)` function (empty revert / panic).
///
/// This is the most common revert pattern in Solidity contracts (20+ sites in ERC20).
/// Each inlined site generates sbrk(0, 0) + ptrtoint + seal_return (~8 PVM instructions).
/// By outlining into a zero-argument noreturn function, each call site becomes a single
/// `call @__revive_revert_0` (~2 PVM instructions).
pub struct RevertEmpty;

impl RuntimeFunction for RevertEmpty {
    const NAME: &'static str = "__revive_revert_0";

    const ATTRIBUTES: &'static [Attribute] = &[Attribute::NoReturn, Attribute::NoFree];

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.void_type().fn_type(&[], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let zero = context.xlen_type().const_zero();
        // Use unchecked GEP: revert data at offset 0 is always within the
        // static heap. The sbrk overhead is unnecessary since the data was
        // already written to memory before this function is called.
        let heap_pointer = context.build_heap_gep_unchecked(zero)?;
        let offset_pointer = context.builder().build_ptr_to_int(
            heap_pointer.value,
            context.xlen_type(),
            "return_data_ptr_to_int",
        )?;
        let flags = context.integer_const(crate::polkavm::XLEN, 1);

        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::RETURN,
            &[flags.into(), offset_pointer.into(), zero.into()],
        );
        context.build_unreachable();

        Ok(None)
    }
}

impl WriteLLVM for RevertEmpty {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Outlined `revert(0, length)` function for non-zero constant lengths.
///
/// After error data has been stored to heap memory at offset 0, this function
/// calls seal_return(1, heap_base, length). Used for revert(0, 4), revert(0, 36),
/// revert(0, 68), revert(0, 100) patterns. ERC20 has 34+ such sites.
pub struct Revert;

impl RuntimeFunction for Revert {
    const NAME: &'static str = "__revive_revert";

    const ATTRIBUTES: &'static [Attribute] = &[Attribute::NoReturn, Attribute::NoFree];

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context
            .void_type()
            .fn_type(&[context.xlen_type().into()], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let length = Self::paramater(context, 0).into_int_value();

        let zero = context.xlen_type().const_zero();
        let heap_pointer = context.build_heap_gep(zero, length)?;
        let offset_pointer = context.builder().build_ptr_to_int(
            heap_pointer.value,
            context.xlen_type(),
            "return_data_ptr_to_int",
        )?;
        let flags = context.integer_const(crate::polkavm::XLEN, 1);

        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::RETURN,
            &[flags.into(), offset_pointer.into(), length.into()],
        );
        context.build_unreachable();

        Ok(None)
    }
}

impl WriteLLVM for Revert {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Outlined `Panic(uint256)` revert helper: writes the Panic ABI encoding to
/// scratch memory (selector at offset 0, code at offset 4) and reverts with
/// length 36. The code is passed as an `i256` argument so a single helper
/// covers every panic code (0x01, 0x11, 0x12, 0x21, 0x22, 0x31, 0x32, 0x41).
/// Replaces the inline two-mstore + revert pattern at every panic site, which
/// otherwise duplicates ~30+ instructions across every function that traps.
pub struct RevertPanic;

impl RuntimeFunction for RevertPanic {
    const NAME: &'static str = "__revive_panic";

    const ATTRIBUTES: &'static [Attribute] = &[Attribute::NoReturn, Attribute::NoFree];

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context
            .void_type()
            .fn_type(&[context.word_type().into()], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let code = Self::paramater(context, 0).into_int_value();

        let zero_xlen = context.xlen_type().const_zero();
        let four_xlen = context.xlen_type().const_int(4, false);
        let panic_selector =
            context.word_const_str_hex(revive_common::PANIC_UINT256_SELECTOR_WORD_HEX);

        crate::polkavm::evm::memory::store_bswap_unchecked(context, zero_xlen, panic_selector)?;
        crate::polkavm::evm::memory::store_bswap_unchecked(context, four_xlen, code)?;

        let length = context.xlen_type().const_int(0x24, false);
        let heap_pointer = context.build_heap_gep_unchecked(zero_xlen)?;
        let offset_pointer = context.builder().build_ptr_to_int(
            heap_pointer.value,
            context.xlen_type(),
            "panic_data_ptr_to_int",
        )?;
        let flags = context.integer_const(crate::polkavm::XLEN, 1);

        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::RETURN,
            &[flags.into(), offset_pointer.into(), length.into()],
        );
        context.build_unreachable();

        Ok(None)
    }
}

impl WriteLLVM for RevertPanic {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}
