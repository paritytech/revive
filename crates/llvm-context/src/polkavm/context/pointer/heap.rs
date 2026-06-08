//! The revive simulated EVM linear memory pointer functions.

use std::num::NonZeroU32;

use inkwell::values::BasicValueEnum;

use revive_common::BYTE_LENGTH_BYTE;
use revive_common::BYTE_LENGTH_WORD;
use revive_common::BYTE_LENGTH_X64;

use crate::polkavm::context::attribute::MemoryEffect;
use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::WriteLLVM;

/// Load a word size value from a heap pointer.
/// Uses 4x 64-bit loads with bswap for efficient byte-order conversion.
pub struct LoadWord;

impl RuntimeFunction for LoadWord {
    const NAME: &'static str = "__revive_load_heap_word";

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context
            .word_type()
            .fn_type(&[context.xlen_type().into()], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        let offset = Self::paramater(context, 0).into_int_value();
        let length = context
            .xlen_type()
            .const_int(BYTE_LENGTH_WORD as u64, false);
        let pointer = context.build_heap_gep(offset, length)?;
        let result = build_efficient_load_swap(context, pointer.value)?;
        Ok(Some(result))
    }
}

impl WriteLLVM for LoadWord {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Store a word size value through a heap pointer.
/// Uses 4x 64-bit stores with bswap for efficient byte-order conversion.
pub struct StoreWord;

impl RuntimeFunction for StoreWord {
    const NAME: &'static str = "__revive_store_heap_word";

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.void_type().fn_type(
            &[context.xlen_type().into(), context.word_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        let offset = Self::paramater(context, 0).into_int_value();
        let length = context
            .xlen_type()
            .const_int(BYTE_LENGTH_WORD as u64, false);
        let pointer = context.build_heap_gep(offset, length)?;
        let value = Self::paramater(context, 1).into_int_value();
        build_efficient_store_swap(context, pointer.value, value)?;
        Ok(None)
    }
}

/// Builds an efficient 256-bit load with byte-swap using 4x 64-bit operations.
///
/// This is much more efficient than LLVM's default lowering of llvm.bswap.i256
/// which generates byte-by-byte loads. We instead:
/// 1. Load 4x 64-bit values
/// 2. Byte-swap each 64-bit value using llvm.bswap.i64
/// 3. Combine them in reversed word order to form the 256-bit result
pub(crate) fn build_efficient_load_swap<'ctx>(
    context: &Context<'ctx>,
    pointer: inkwell::values::PointerValue<'ctx>,
) -> anyhow::Result<BasicValueEnum<'ctx>> {
    let i64_type = context
        .llvm()
        .custom_width_int_type(NonZeroU32::new(64).expect("const is non-zero"))
        .expect("valid integer width");
    let i8_type = context
        .llvm()
        .custom_width_int_type(NonZeroU32::new(8).expect("const is non-zero"))
        .expect("valid integer width");
    let word_type = context.word_type();

    let bswap64 = inkwell::intrinsics::Intrinsic::find("llvm.bswap.i64")
        .expect("ICE: llvm.bswap.i64 intrinsic exists");
    let bswap64_function = bswap64
        .get_declaration(context.module(), &[i64_type.into()])
        .expect("ICE: bswap.i64 declaration");

    let mut swapped_x64_values = Vec::with_capacity(4);
    for index in 0..4 {
        let gep_offset = context
            .xlen_type()
            .const_int(index * BYTE_LENGTH_X64 as u64, false);
        let byte_pointer = unsafe {
            context.builder().build_gep(
                i8_type,
                pointer,
                &[gep_offset],
                &format!("byte_pointer_{index}"),
            )?
        };
        let x64_value = context
            .builder()
            .build_load(i64_type, byte_pointer, &format!("x64_value_{index}"))?
            .into_int_value();
        context
            .basic_block()
            .get_last_instruction()
            .expect("ICE: load instruction always exists")
            .set_alignment(BYTE_LENGTH_BYTE as u32)
            .expect("ICE: alignment is valid");

        let swapped_x64_value = context
            .builder()
            .build_call(
                bswap64_function,
                &[x64_value.into()],
                &format!("swapped_x64_value_{index}"),
            )?
            .try_as_basic_value()
            .unwrap_basic()
            .into_int_value();
        swapped_x64_values.push(swapped_x64_value);
    }

    // Combine in reversed register order: chunks 3, 2, 1, 0 from most to
    // least significant. This achieves the full big-endian to little-endian
    // conversion of the 256-bit word.
    let mut result =
        context
            .builder()
            .build_int_z_extend(swapped_x64_values[3], word_type, "ext_0")?;
    for (index, &x64_value) in swapped_x64_values.iter().rev().skip(1).enumerate() {
        let extended = context.builder().build_int_z_extend(
            x64_value,
            word_type,
            &format!("ext_{}", index + 1),
        )?;
        let shift_amount = word_type.const_int(64 * (index + 1) as u64, false);
        let shifted = context.builder().build_left_shift(
            extended,
            shift_amount,
            &format!("shift_{}", index + 1),
        )?;
        result = context
            .builder()
            .build_or(result, shifted, &format!("or_{}", index + 1))?;
    }

    Ok(result.into())
}

/// Builds an efficient 256-bit store with byte-swap using 4x 64-bit operations.
///
/// This is much more efficient than LLVM's default lowering of llvm.bswap.i256
/// which generates byte-by-byte stores. We instead:
/// 1. Extract 4x 64-bit values from the 256-bit word
/// 2. Byte-swap each 64-bit value using llvm.bswap.i64
/// 3. Store them in reversed word order
pub(crate) fn build_efficient_store_swap<'ctx>(
    context: &Context<'ctx>,
    pointer: inkwell::values::PointerValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()> {
    let i64_type = context
        .llvm()
        .custom_width_int_type(NonZeroU32::new(64).expect("const is non-zero"))
        .expect("valid integer width");
    let i8_type = context
        .llvm()
        .custom_width_int_type(NonZeroU32::new(8).expect("const is non-zero"))
        .expect("valid integer width");
    let word_type = context.word_type();

    let bswap64 = inkwell::intrinsics::Intrinsic::find("llvm.bswap.i64")
        .expect("ICE: llvm.bswap.i64 intrinsic exists");
    let bswap64_function = bswap64
        .get_declaration(context.module(), &[i64_type.into()])
        .expect("ICE: bswap.i64 declaration");

    for index in 0..4 {
        let shift_amount = word_type.const_int(64 * index as u64, false);
        let shifted = context.builder().build_right_shift(
            value,
            shift_amount,
            false,
            &format!("shift_{index}"),
        )?;
        let x64_value =
            context
                .builder()
                .build_int_truncate(shifted, i64_type, &format!("trunc_{index}"))?;

        let swapped_x64_value = context
            .builder()
            .build_call(
                bswap64_function,
                &[x64_value.into()],
                &format!("swap_x64_value_{index}"),
            )?
            .try_as_basic_value()
            .unwrap_basic()
            .into_int_value();

        // Reverse the position so the least-significant chunk lands at the
        // highest byte offset; this performs the big-endian byte order on
        // the 256-bit word as a whole.
        let store_byte_offset = (3 - index) * BYTE_LENGTH_X64;
        let gep_offset = context
            .xlen_type()
            .const_int(store_byte_offset as u64, false);
        let byte_pointer = unsafe {
            context.builder().build_gep(
                i8_type,
                pointer,
                &[gep_offset],
                &format!("store_pointer_{index}"),
            )?
        };
        let store_instruction = context
            .builder()
            .build_store(byte_pointer, swapped_x64_value)?;
        store_instruction
            .set_alignment(BYTE_LENGTH_BYTE as u32)
            .expect("ICE: alignment is valid");
    }

    Ok(())
}

impl WriteLLVM for StoreWord {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Load a word size value from a heap pointer without byte-swapping.
/// Used for internal memory operations that don't escape to external code.
pub struct LoadWordNative;

impl RuntimeFunction for LoadWordNative {
    const NAME: &'static str = "__revive_load_heap_word_native";

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context
            .word_type()
            .fn_type(&[context.xlen_type().into()], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        let offset = Self::paramater(context, 0).into_int_value();
        let length = context
            .xlen_type()
            .const_int(BYTE_LENGTH_WORD as u64, false);
        let pointer = context.build_heap_gep(offset, length)?;
        let value = context
            .builder()
            .build_load(context.word_type(), pointer.value, "value")?;
        context
            .basic_block()
            .get_last_instruction()
            .expect("ICE: load instruction always exists")
            .set_alignment(BYTE_LENGTH_BYTE as u32)
            .expect("ICE: alignment is valid");
        Ok(Some(value))
    }
}

impl WriteLLVM for LoadWordNative {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Store a word size value through a heap pointer without byte-swapping.
/// Used for internal memory operations that don't escape to external code.
pub struct StoreWordNative;

impl RuntimeFunction for StoreWordNative {
    const NAME: &'static str = "__revive_store_heap_word_native";

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.void_type().fn_type(
            &[context.xlen_type().into(), context.word_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        let offset = Self::paramater(context, 0).into_int_value();
        let length = context
            .xlen_type()
            .const_int(BYTE_LENGTH_WORD as u64, false);
        let pointer = context.build_heap_gep(offset, length)?;
        let value = Self::paramater(context, 1);

        context
            .builder()
            .build_store(pointer.value, value)?
            .set_alignment(BYTE_LENGTH_BYTE as u32)
            .expect("ICE: alignment is valid");
        Ok(None)
    }
}

impl WriteLLVM for StoreWordNative {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Keccak256 hash of one 256-bit word from scratch memory.
/// Equivalent to: mstore(0, word0); sha3(0, 32)
/// Deduplicated into a single function to reduce code size for storage slot lookups.
pub struct Keccak256OneWord;

impl Keccak256OneWord {
    /// The function name.
    pub const NAME: &'static str = "__revive_keccak256_one_word";
}

impl RuntimeFunction for Keccak256OneWord {
    const NAME: &'static str = "__revive_keccak256_one_word";

    const ATTRIBUTES: &'static [crate::polkavm::context::Attribute] = &[
        crate::polkavm::context::Attribute::NoFree,
        crate::polkavm::context::Attribute::NoRecurse,
        crate::polkavm::context::Attribute::WillReturn,
        crate::polkavm::context::Attribute::NoInline,
    ];

    /// Writes the input back to heap[0..32] (matching EVM's
    /// `mstore(0, word0); keccak256(0, 32)`), then hashes from there.
    /// This lets `mem_opt`'s keccak fusion safely dead-eliminate the
    /// preceding `mstore(0, k)` — a later `mload(0)` whose mem_opt
    /// forwarding has been invalidated (e.g., by an intervening
    /// external call) reads the helper's heap write and gets the
    /// same value EVM would. The helper is no longer
    /// `MemoryEffect::None`; `Unrestricted` gives LLVM accurate
    /// information about its memory effects.
    const MEMORY_EFFECT: MemoryEffect = MemoryEffect::Unrestricted;

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context
            .word_type()
            .fn_type(&[context.word_type().into()], false)
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        let word0 = Self::paramater(context, 0).into_int_value();

        // Mirror EVM's `mstore(0, word0)`: byte-swap and store to heap[0..32].
        let zero_xlen = context.xlen_type().const_zero();
        crate::polkavm::evm::memory::store_bswap_unchecked(context, zero_xlen, word0)?;

        let length = context
            .xlen_type()
            .const_int(BYTE_LENGTH_WORD as u64, false);

        let output_pointer = context.build_alloca_at_entry(context.word_type(), "output_pointer");
        let input_pointer = context.build_heap_gep_unchecked(zero_xlen)?;

        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::HASH_KECCAK_256,
            &[
                input_pointer.to_int(context).into(),
                length.into(),
                output_pointer.to_int(context).into(),
            ],
        );

        let result = context.build_byte_swap(context.build_load(output_pointer, "sha3_output")?)?;
        Ok(Some(result))
    }
}

impl WriteLLVM for Keccak256OneWord {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}

/// Keccak256 hash of two 256-bit words from scratch memory.
/// Equivalent to: mstore(0, word0); mstore(32, word1); sha3(0, 64)
/// Deduplicated into a single function to reduce code size for mapping lookups.
pub struct Keccak256TwoWords;

impl Keccak256TwoWords {
    /// The function name.
    pub const NAME: &'static str = "__revive_keccak256_two_words";
}

impl RuntimeFunction for Keccak256TwoWords {
    const NAME: &'static str = "__revive_keccak256_two_words";

    const ATTRIBUTES: &'static [crate::polkavm::context::Attribute] = &[
        crate::polkavm::context::Attribute::NoFree,
        crate::polkavm::context::Attribute::NoRecurse,
        crate::polkavm::context::Attribute::WillReturn,
        crate::polkavm::context::Attribute::NoInline,
    ];

    /// Writes the inputs back to heap[0..64] (matching EVM's
    /// `mstore(0, word0); mstore(32, word1); keccak256(0, 64)`), then
    /// hashes from there. This lets `mem_opt`'s keccak fusion safely
    /// dead-eliminate the preceding mstores — a later `mload` at offset
    /// 0 or 32 whose mem_opt forwarding has been invalidated reads the
    /// helper's heap writes and gets the same value EVM would.
    const MEMORY_EFFECT: MemoryEffect = MemoryEffect::Unrestricted;

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.word_type().fn_type(
            &[context.word_type().into(), context.word_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<BasicValueEnum<'ctx>>> {
        let word0 = Self::paramater(context, 0).into_int_value();
        let word1 = Self::paramater(context, 1).into_int_value();

        // Mirror EVM's `mstore(0, word0); mstore(32, word1)`: byte-swap
        // and store both words to heap[0..64].
        let zero_xlen = context.xlen_type().const_zero();
        let word_xlen = context
            .xlen_type()
            .const_int(BYTE_LENGTH_WORD as u64, false);
        crate::polkavm::evm::memory::store_bswap_unchecked(context, zero_xlen, word0)?;
        crate::polkavm::evm::memory::store_bswap_unchecked(context, word_xlen, word1)?;

        let length = context
            .xlen_type()
            .const_int(2 * BYTE_LENGTH_WORD as u64, false);

        let output_pointer = context.build_alloca_at_entry(context.word_type(), "output_pointer");
        let input_pointer = context.build_heap_gep_unchecked(zero_xlen)?;

        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::HASH_KECCAK_256,
            &[
                input_pointer.to_int(context).into(),
                length.into(),
                output_pointer.to_int(context).into(),
            ],
        );

        let result = context.build_byte_swap(context.build_load(output_pointer, "sha3_output")?)?;
        Ok(Some(result))
    }
}

impl WriteLLVM for Keccak256TwoWords {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}
