//! Translates the arithmetic operations.

use inkwell::values::BasicValue;

use crate::polkavm::context::runtime::RuntimeFunction;
use crate::polkavm::context::Context;
use crate::polkavm::context::Pointer;
use crate::polkavm::Dependency;
use crate::polkavm::WriteLLVM;

const SOLIDITY_TRANSFER_GAS_STIPEND_THRESHOLD: u64 = 2300;

/// The Solidity `address.transfer` and `address.send` call detection heuristic.
///
/// # Why
/// This heuristic is an additional security feature to guard against re-entrancy attacks
/// in case contract authors violate Solidity best practices and use `address.transfer` or
/// `address.send`.
/// While contract authors are supposed to never use `address.transfer` or `address.send`,
/// for a small cost we can be extra defensive about it.
///
/// # How
/// The gas stipend emitted by solc for `transfer` and `send` is not static, thus:
/// - Dynamically allow re-entrancy only for calls considered not transfer or send.
/// - Detected balance transfers will supply 0 deposit limit instead of `u256::MAX`.
///
/// Calls are considered transfer or send if:
/// - (Input length | Output lenght) == 0;
/// - Gas <= 2300;
///
/// # Arguments:
/// - The deposit value pointer.
/// - The gas value.
/// - `input_length | output_length`.
///
///
/// # Returns:
/// The call flags xlen `IntValue`
pub struct CallReentrancyProtector;

impl<D> RuntimeFunction<D> for CallReentrancyProtector
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_call_reentrancy_protector";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.xlen_type().fn_type(
            &[
                context.llvm().ptr_type(Default::default()).into(),
                context.word_type().into(),
                context.xlen_type().into(),
            ],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let deposit_pointer = Self::paramater(context, 0).into_pointer_value();
        let gas = Self::paramater(context, 1).into_int_value();
        let input_length_or_output_length = Self::paramater(context, 2).into_int_value();

        // Branch-free SSA implementation: First derive the heuristic boolean (int1) value.
        let is_no_input_no_output = context.builder().build_int_compare(
            inkwell::IntPredicate::EQ,
            context.xlen_type().const_zero(),
            input_length_or_output_length,
            "is_no_input_no_output",
        )?;
        let gas_stipend = context
            .word_type()
            .const_int(SOLIDITY_TRANSFER_GAS_STIPEND_THRESHOLD, false);
        let is_gas_stipend_for_transfer_or_send = context.builder().build_int_compare(
            inkwell::IntPredicate::ULE,
            gas,
            gas_stipend,
            "is_gas_stipend_for_transfer_or_send",
        )?;
        let is_balance_transfer = context.builder().build_and(
            is_no_input_no_output,
            is_gas_stipend_for_transfer_or_send,
            "is_balance_transfer",
        )?;
        let is_regular_call = context
            .builder()
            .build_not(is_balance_transfer, "is_balance_transfer_inverted")?;

        // Call flag: Left shift the heuristic boolean value.
        let is_regular_call_xlen = context.builder().build_int_z_extend(
            is_regular_call,
            context.xlen_type(),
            "is_balance_transfer_xlen",
        )?;
        let call_flags = context.builder().build_left_shift(
            is_regular_call_xlen,
            context.xlen_type().const_int(3, false),
            "flags",
        )?;

        // Deposit limit value: Sign-extended the heuristic boolean value.
        let deposit_limit_value = context.builder().build_int_s_extend(
            is_regular_call,
            context.word_type(),
            "deposit_limit_value",
        )?;
        context
            .builder()
            .build_store(deposit_pointer, deposit_limit_value)?;

        Ok(Some(call_flags.into()))
    }
}

impl<D> WriteLLVM<D> for CallReentrancyProtector
where
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::emit(&self, context)
    }
}

/// Implements the CALL operator according to the EVM specification.
///
/// # Arguments:
/// - The address value.
/// - The value value.
/// - The input offset.
/// - The input length.
/// - The output offset.
/// - The output length.
/// - The deposit limit pointer.
/// - The call flags.
///
/// # Returns:
/// - The success value (as xlen)
pub struct Call;

impl<D> RuntimeFunction<D> for Call
where
    D: Dependency + Clone,
{
    const NAME: &'static str = "__revive_call";

    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx> {
        context.register_type().fn_type(
            &[
                context.word_type().into(),
                context.word_type().into(),
                context.xlen_type().into(),
                context.xlen_type().into(),
                context.xlen_type().into(),
                context.xlen_type().into(),
                context.llvm().ptr_type(Default::default()).into(),
                context.xlen_type().into(),
            ],
            false,
        )
    }

    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>> {
        let address = Self::paramater(context, 0).into_int_value();
        let value = Self::paramater(context, 1).into_int_value();
        let input_offset = Self::paramater(context, 2).into_int_value();
        let input_length = Self::paramater(context, 3).into_int_value();
        let output_offset = Self::paramater(context, 4).into_int_value();
        let output_length = Self::paramater(context, 5).into_int_value();
        let depsit_limit_pointer = Self::paramater(context, 6).into_pointer_value();
        let flags = Self::paramater(context, 7).into_int_value();

        let address_pointer = context.build_address_argument_store(address)?;

        let value_pointer = context.build_alloca_at_entry(context.word_type(), "value_pointer");
        context.build_store(value_pointer, value)?;

        let input_pointer = context.build_heap_gep(input_offset, input_length)?;
        let output_pointer = context.build_heap_gep(output_offset, output_length)?;

        let output_length_pointer =
            context.build_alloca_at_entry(context.xlen_type(), "output_length");
        context.build_store(output_length_pointer, output_length)?;

        let flags_and_callee = revive_runtime_api::calling_convention::pack_hi_lo_reg(
            context.builder(),
            context.llvm(),
            flags,
            address_pointer.to_int(context),
            "address_and_callee",
        )?;
        let deposit_limit_pointer = Pointer::new(
            context.word_type(),
            Default::default(),
            depsit_limit_pointer,
        );
        let deposit_and_value = revive_runtime_api::calling_convention::pack_hi_lo_reg(
            context.builder(),
            context.llvm(),
            deposit_limit_pointer.to_int(context),
            value_pointer.to_int(context),
            "deposit_and_value",
        )?;
        let input_data = revive_runtime_api::calling_convention::pack_hi_lo_reg(
            context.builder(),
            context.llvm(),
            input_length,
            input_pointer.to_int(context),
            "input_data",
        )?;
        let output_data = revive_runtime_api::calling_convention::pack_hi_lo_reg(
            context.builder(),
            context.llvm(),
            output_length_pointer.to_int(context),
            output_pointer.to_int(context),
            "output_data",
        )?;

        let name = revive_runtime_api::polkavm_imports::CALL;
        let success = context
            .build_runtime_call(
                name,
                &[
                    flags_and_callee.into(),
                    context.register_type().const_all_ones().into(),
                    context.register_type().const_all_ones().into(),
                    deposit_and_value.into(),
                    input_data.into(),
                    output_data.into(),
                ],
            )
            .unwrap_or_else(|| panic!("{name} should return a value"))
            .into_int_value();
        Ok(Some(success.into()))
    }
}

impl<D> WriteLLVM<D> for Call
where
    D: Dependency + Clone,
{
    fn declare(&mut self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::declare(self, context)
    }

    fn into_llvm(self, context: &mut Context<D>) -> anyhow::Result<()> {
        <Self as RuntimeFunction<_>>::emit(&self, context)
    }
}
