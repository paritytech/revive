//! Emulates the linear EVM heap memory via a simulated `sbrk` system call.

use inkwell::values::BasicValue;
use revive_common::BYTE_LENGTH_WORD;

use crate::polkavm::context::attribute::Attribute;
use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::WriteLLVM;

/// Simulates the `sbrk` system call, reproducing the semantics of the EVM heap memory.
///
/// Parameters:
/// - The `offset` into the emulated EVM heap memory.
/// - The `size` of the allocation emulated EVM heap memory.
///
/// Returns:
/// - A pointer to the EVM heap memory at given `offset`.
///
/// Semantics:
/// - Traps if the offset is out of bounds.
/// - Aligns the total heap memory size to the EVM word size.
/// - Traps if the memory size would be greater than the configured EVM heap memory size.
/// - Maintains the total memory size (`msize`) in global heap size value.
pub struct Sbrk;

impl RuntimeFunction for Sbrk {
    const NAME: &'static str = "__sbrk_internal";

    const ATTRIBUTES: &'static [Attribute] = &[
        Attribute::NoFree,
        Attribute::NoRecurse,
        Attribute::WillReturn,
    ];

    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx> {
        context.llvm().ptr_type(Default::default()).fn_type(
            &[context.xlen_type().into(), context.xlen_type().into()],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let offset = Self::paramater(context, 0).into_int_value();
        let size = Self::paramater(context, 1).into_int_value();

        let return_block = context.append_basic_block("return_pointer");
        let body_block = context.append_basic_block("body");
        let is_size_zero = context.builder().build_int_compare(
            inkwell::IntPredicate::EQ,
            size,
            context.xlen_type().const_zero(),
            "is_size_zero",
        )?;
        context.build_conditional_branch(is_size_zero, return_block, body_block)?;

        context.set_basic_block(body_block);
        let trap_block = context.append_basic_block("trap");
        let offset_in_bounds_block = context.append_basic_block("offset_in_bounds");
        let is_offset_out_of_bounds = context.builder().build_int_compare(
            inkwell::IntPredicate::UGE,
            offset,
            context.heap_size(),
            "offset_out_of_bounds",
        )?;
        context.build_conditional_branch(
            is_offset_out_of_bounds,
            trap_block,
            offset_in_bounds_block,
        )?;

        context.set_basic_block(trap_block);
        context.build_call(context.intrinsics().trap, &[], "invalid_trap");
        context.build_unreachable();

        context.set_basic_block(offset_in_bounds_block);
        let size_in_bounds_block = context.append_basic_block("size_in_bounds");
        let is_size_out_of_bounds = context.builder().build_int_compare(
            inkwell::IntPredicate::UGT,
            size,
            context.heap_size(),
            "size_in_bounds",
        )?;
        context.build_conditional_branch(
            is_size_out_of_bounds,
            trap_block,
            size_in_bounds_block,
        )?;

        context.set_basic_block(size_in_bounds_block);
        let mask = context
            .xlen_type()
            .const_int(BYTE_LENGTH_WORD as u64 - 1, false);
        let total_size = context
            .builder()
            .build_int_add(offset, size, "total_size")?;
        let memory_size = context.builder().build_and(
            context.builder().build_int_add(total_size, mask, "mask")?,
            context.builder().build_not(mask, "mask_not")?,
            "memory_size",
        )?;
        let total_size_in_bounds_block = context.append_basic_block("total_size_in_bounds");
        let is_total_size_out_of_bounds = context.builder().build_int_compare(
            inkwell::IntPredicate::UGT,
            memory_size,
            context.heap_size(),
            "size_out_of_bounds",
        )?;
        context.build_conditional_branch(
            is_total_size_out_of_bounds,
            trap_block,
            total_size_in_bounds_block,
        )?;

        context.set_basic_block(total_size_in_bounds_block);
        let new_size_block = context.append_basic_block("new_size");
        let is_new_size = context.builder().build_int_compare(
            inkwell::IntPredicate::UGT,
            memory_size,
            context
                .get_global_value(crate::polkavm::GLOBAL_HEAP_SIZE)?
                .into_int_value(),
            "is_new_size",
        )?;
        context.build_conditional_branch(is_new_size, new_size_block, return_block)?;

        context.set_basic_block(new_size_block);
        context.build_store(
            context.get_global(crate::polkavm::GLOBAL_HEAP_SIZE)?.into(),
            memory_size,
        )?;
        context.build_unconditional_branch(return_block);

        context.set_basic_block(return_block);
        Ok(Some(
            context
                .build_gep(
                    context
                        .get_global(crate::polkavm::GLOBAL_HEAP_MEMORY)?
                        .into(),
                    &[context.xlen_type().const_zero(), offset],
                    context.byte_type(),
                    "allocation_start_pointer",
                )
                .value
                .as_basic_value_enum(),
        ))
    }
}

impl WriteLLVM for Sbrk {
    fn declare(&mut self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context) -> anyhow::Result<()> {
        <Self as RuntimeFunction>::emit(&self, context)
    }
}
