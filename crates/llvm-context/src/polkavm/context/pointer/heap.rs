//! The revive simulated EVM linear memory pointer functions.

use inkwell::values::BasicValueEnum;

use revive_common::BYTE_LENGTH_BYTE;
use revive_common::BYTE_LENGTH_WORD;

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::WriteLLVM;

/// Size of a 64-bit word in bytes.
const BYTE_LENGTH_QWORD: usize = 8;

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

        // Use efficient 4x64-bit loads with word shuffling
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

        // Use efficient 4x64-bit stores with word shuffling
        build_efficient_store_swap(context, pointer.value, value)?;
        Ok(None)
    }
}

/// Builds an efficient 256-bit store with byte-swap at a pointer obtained via
/// unchecked GEP (no sbrk bounds check). For use by the newyork InlineByteSwap
/// mode on constant offsets within the static heap.
pub fn store_bswap_unchecked<'ctx>(
    context: &Context<'ctx>,
    offset: inkwell::values::IntValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()> {
    let pointer = context.build_heap_gep_unchecked(offset)?;
    build_efficient_store_swap(context, pointer.value, value)
}

/// Builds an efficient 256-bit load with byte-swap at a pointer obtained via
/// unchecked GEP (no sbrk bounds check). For use by the newyork InlineByteSwap
/// mode on constant offsets within the static heap.
pub fn load_bswap_unchecked<'ctx>(
    context: &Context<'ctx>,
    offset: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<BasicValueEnum<'ctx>> {
    let pointer = context.build_heap_gep_unchecked(offset)?;
    build_efficient_load_swap(context, pointer.value)
}

/// Builds an efficient 256-bit load with byte-swap using 4x 64-bit operations.
///
/// This is much more efficient than LLVM's default lowering of llvm.bswap.i256
/// which generates byte-by-byte loads. We instead:
/// 1. Load 4x 64-bit values
/// 2. Byte-swap each 64-bit value using llvm.bswap.i64
/// 3. Combine them in reversed word order to form the 256-bit result
fn build_efficient_load_swap<'ctx>(
    context: &Context<'ctx>,
    pointer: inkwell::values::PointerValue<'ctx>,
) -> anyhow::Result<BasicValueEnum<'ctx>> {
    let i64_type = context.llvm().custom_width_int_type(64);
    let i8_type = context.llvm().custom_width_int_type(8);
    let word_type = context.word_type();

    // Get the bswap.i64 intrinsic
    let bswap64 = inkwell::intrinsics::Intrinsic::find("llvm.bswap.i64")
        .expect("llvm.bswap.i64 intrinsic exists");
    let bswap64_fn = bswap64
        .get_declaration(context.module(), &[i64_type.into()])
        .expect("bswap.i64 declaration");

    // Load 4 qwords at byte offsets 0, 8, 16, 24 and byte-swap each
    // Use i8 as element type so GEP offset is in bytes
    let mut swapped = Vec::with_capacity(4);
    for i in 0..4 {
        let gep_offset = context
            .xlen_type()
            .const_int(i * BYTE_LENGTH_QWORD as u64, false);
        let byte_ptr = unsafe {
            context.builder().build_gep(
                i8_type,
                pointer,
                &[gep_offset],
                &format!("qword_ptr_{i}"),
            )?
        };
        let qword = context
            .builder()
            .build_load(i64_type, byte_ptr, &format!("qword_{i}"))?
            .into_int_value();
        // Set alignment to 8 bytes to allow efficient 64-bit loads
        context
            .basic_block()
            .get_last_instruction()
            .expect("Always exists")
            .set_alignment(BYTE_LENGTH_QWORD as u32)
            .expect("Alignment is valid");

        // Byte-swap the 64-bit value
        let swapped_qword = context
            .builder()
            .build_call(bswap64_fn, &[qword.into()], &format!("swapped_{i}"))?
            .try_as_basic_value()
            .unwrap_basic()
            .into_int_value();
        swapped.push(swapped_qword);
    }

    // Combine in reversed word order: qword[3], qword[2], qword[1], qword[0]
    // This achieves the full big-endian to little-endian conversion
    let mut result = context
        .builder()
        .build_int_z_extend(swapped[3], word_type, "ext_0")?;
    for (i, &qword) in swapped.iter().rev().skip(1).enumerate() {
        let extended =
            context
                .builder()
                .build_int_z_extend(qword, word_type, &format!("ext_{}", i + 1))?;
        let shift_amount = word_type.const_int(64 * (i + 1) as u64, false);
        let shifted = context.builder().build_left_shift(
            extended,
            shift_amount,
            &format!("shift_{}", i + 1),
        )?;
        result = context
            .builder()
            .build_or(result, shifted, &format!("or_{}", i + 1))?;
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
fn build_efficient_store_swap<'ctx>(
    context: &Context<'ctx>,
    pointer: inkwell::values::PointerValue<'ctx>,
    value: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<()> {
    let i64_type = context.llvm().custom_width_int_type(64);
    let i8_type = context.llvm().custom_width_int_type(8);
    let word_type = context.word_type();

    // Get the bswap.i64 intrinsic
    let bswap64 = inkwell::intrinsics::Intrinsic::find("llvm.bswap.i64")
        .expect("llvm.bswap.i64 intrinsic exists");
    let bswap64_fn = bswap64
        .get_declaration(context.module(), &[i64_type.into()])
        .expect("bswap.i64 declaration");

    // Extract 4 qwords, byte-swap each, and store in reversed order
    for i in 0..4 {
        // Extract the i-th qword (from least significant)
        let shift_amount = word_type.const_int(64 * i as u64, false);
        let shifted = context.builder().build_right_shift(
            value,
            shift_amount,
            false,
            &format!("shift_{i}"),
        )?;
        let qword =
            context
                .builder()
                .build_int_truncate(shifted, i64_type, &format!("trunc_{i}"))?;

        // Byte-swap the 64-bit value
        let swapped_qword = context
            .builder()
            .build_call(bswap64_fn, &[qword.into()], &format!("swap_{i}"))?
            .try_as_basic_value()
            .unwrap_basic()
            .into_int_value();

        // Store at reversed position: qword 0 goes to byte offset 24, qword 3 goes to byte offset 0
        // Use i8 as element type so GEP offset is in bytes
        let store_byte_offset = (3 - i) * BYTE_LENGTH_QWORD;
        let gep_offset = context
            .xlen_type()
            .const_int(store_byte_offset as u64, false);
        let byte_ptr = unsafe {
            context.builder().build_gep(
                i8_type,
                pointer,
                &[gep_offset],
                &format!("store_ptr_{i}"),
            )?
        };
        let store_inst = context.builder().build_store(byte_ptr, swapped_qword)?;
        // Set alignment to 8 bytes to allow efficient 64-bit stores
        store_inst
            .set_alignment(BYTE_LENGTH_QWORD as u32)
            .expect("Alignment is valid");
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
            .expect("Always exists")
            .set_alignment(BYTE_LENGTH_BYTE as u32)
            .expect("Alignment is valid");

        // No byte-swap for native operations
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

        // No byte-swap for native operations
        let value = Self::paramater(context, 1);

        context
            .builder()
            .build_store(pointer.value, value)?
            .set_alignment(BYTE_LENGTH_BYTE as u32)
            .expect("Alignment is valid");
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

        // Store word0 at heap offset 0 using inline bswap (no sbrk needed)
        let offset0 = context.xlen_type().const_int(0, false);
        store_bswap_unchecked(context, offset0, word0)?;

        // Get heap pointer directly (offset 0 is always within static heap)
        let input_pointer = context.build_heap_gep_unchecked(offset0)?;
        let length = context
            .xlen_type()
            .const_int(BYTE_LENGTH_WORD as u64, false);

        // Allocate output on stack
        let output_pointer = context.build_alloca_at_entry(context.word_type(), "output_pointer");

        // Call hash_keccak_256
        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::HASH_KECCAK_256,
            &[
                input_pointer.to_int(context).into(),
                length.into(),
                output_pointer.to_int(context).into(),
            ],
        );

        // Load result and byte-swap
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

        // Store word0 at heap offset 0 using inline bswap (no sbrk needed)
        let offset0 = context.xlen_type().const_int(0, false);
        store_bswap_unchecked(context, offset0, word0)?;

        // Store word1 at heap offset 32 using inline bswap (no sbrk needed)
        let offset32 = context
            .xlen_type()
            .const_int(BYTE_LENGTH_WORD as u64, false);
        store_bswap_unchecked(context, offset32, word1)?;

        // Get heap pointer directly (offset 0 is always within static heap)
        let input_pointer = context.build_heap_gep_unchecked(offset0)?;
        let length = context
            .xlen_type()
            .const_int(2 * BYTE_LENGTH_WORD as u64, false);

        // Allocate output on stack
        let output_pointer = context.build_alloca_at_entry(context.word_type(), "output_pointer");

        // Call hash_keccak_256
        context.build_runtime_call(
            revive_runtime_api::polkavm_imports::HASH_KECCAK_256,
            &[
                input_pointer.to_int(context).into(),
                length.into(),
                output_pointer.to_int(context).into(),
            ],
        );

        // Load result and byte-swap
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
