//!
//! Translates the call instructions of the EraVM Yul extension.
//!

use crate::eravm::context::address_space::AddressSpace;
use crate::eravm::context::function::declaration::Declaration as FunctionDeclaration;
use crate::eravm::context::pointer::Pointer;
use crate::eravm::context::Context;
use crate::eravm::Dependency;

///
/// Generates a mimic call.
///
/// The mimic call is a special type of call that can only be used in the system contracts of
/// zkSync. The call allows to call a contract with custom `msg.sender`, allowing to insert
/// system contracts as middlewares.
///
pub fn mimic<'ctx, D>(
    context: &mut Context<'ctx, D>,
    function: FunctionDeclaration<'ctx>,
    address: inkwell::values::IntValue<'ctx>,
    mimic: inkwell::values::IntValue<'ctx>,
    abi_data: inkwell::values::BasicValueEnum<'ctx>,
    extra_abi_data: Vec<inkwell::values::IntValue<'ctx>>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let status_code_result_pointer = context.build_alloca(
        context.field_type(),
        "mimic_call_result_status_code_pointer",
    );
    context.build_store(status_code_result_pointer, context.field_const(0));

    let far_call_result = context
        .build_call(
            function,
            crate::eravm::utils::external_call_arguments(
                context,
                abi_data,
                address,
                extra_abi_data,
                Some(mimic),
            )
            .as_slice(),
            "mimic_call_external",
        )
        .expect("IntrinsicFunction always returns a flag");

    let result_abi_data = context
        .builder()
        .build_extract_value(
            far_call_result.into_struct_value(),
            0,
            "mimic_call_external_result_abi_data",
        )
        .expect("Always exists");
    let result_abi_data_pointer = Pointer::new(
        context.byte_type(),
        AddressSpace::Generic,
        result_abi_data.into_pointer_value(),
    );

    let result_status_code_boolean = context
        .builder()
        .build_extract_value(
            far_call_result.into_struct_value(),
            1,
            "mimic_call_external_result_status_code_boolean",
        )
        .expect("Always exists");
    let result_status_code = context.builder().build_int_z_extend_or_bit_cast(
        result_status_code_boolean.into_int_value(),
        context.field_type(),
        "mimic_call_external_result_status_code",
    )?;
    context.build_store(status_code_result_pointer, result_status_code);

    context.write_abi_pointer(
        result_abi_data_pointer,
        crate::eravm::GLOBAL_RETURN_DATA_POINTER,
    );
    context.write_abi_data_size(
        result_abi_data_pointer,
        crate::eravm::GLOBAL_RETURN_DATA_SIZE,
    );

    let status_code_result =
        context.build_load(status_code_result_pointer, "mimic_call_status_code");
    Ok(status_code_result)
}

///
/// Generates a raw far call.
///
/// Such calls can accept extra ABI arguments passed via the virtual machine registers.
///
pub fn raw_far<'ctx, D>(
    _context: &mut Context<'ctx, D>,
    _function: FunctionDeclaration<'ctx>,
    _address: inkwell::values::IntValue<'ctx>,
    _abi_data: inkwell::values::BasicValueEnum<'ctx>,
    _output_offset: inkwell::values::IntValue<'ctx>,
    _output_length: inkwell::values::IntValue<'ctx>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    todo!();
    /*
    let status_code_result_pointer = context.build_alloca(
        context.field_type(),
        "system_far_call_result_status_code_pointer",
    );
    context.build_store(status_code_result_pointer, context.field_const(0));

    let far_call_result = context
        .build_call(
            function,
            crate::eravm::utils::external_call_arguments(context, abi_data, address, vec![], None)
                .as_slice(),
            "system_far_call_external",
        )
        .expect("IntrinsicFunction always returns a flag");

    let result_abi_data = context
        .builder()
        .build_extract_value(
            far_call_result.into_struct_value(),
            0,
            "system_far_call_external_result_abi_data",
        )
        .expect("Always exists");
    let result_abi_data_pointer = Pointer::new(
        context.byte_type(),
        AddressSpace::Generic,
        result_abi_data.into_pointer_value(),
    );

    let result_status_code_boolean = context
        .builder()
        .build_extract_value(
            far_call_result.into_struct_value(),
            1,
            "system_far_call_external_result_status_code_boolean",
        )
        .expect("Always exists");
    let result_status_code = context.builder().build_int_z_extend_or_bit_cast(
        result_status_code_boolean.into_int_value(),
        context.field_type(),
        "system_far_call_external_result_status_code",
    );
    context.build_store(status_code_result_pointer, result_status_code);

    let source = result_abi_data_pointer;

    let destination = Pointer::new_with_offset(
        context,
        AddressSpace::Heap,
        context.byte_type(),
        output_offset,
        "system_far_call_destination",
    );

    context.build_memcpy_return_data(
        context.intrinsics().memory_copy_from_generic,
        destination,
        source,
        output_length,
        "system_far_call_memcpy_from_child",
    );

    context.write_abi_pointer(
        result_abi_data_pointer,
        crate::eravm::GLOBAL_RETURN_DATA_POINTER,
    );
    context.write_abi_data_size(
        result_abi_data_pointer,
        crate::eravm::GLOBAL_RETURN_DATA_SIZE,
    );

    let status_code_result =
        context.build_load(status_code_result_pointer, "system_call_status_code");
    Ok(status_code_result)
    */
}

///
/// Generates a system call.
///
/// Such calls can accept extra ABI arguments passed via the virtual machine registers. It is used,
/// for example, to pass the callee address and the Ether value to the `msg.value` simulator.
///
pub fn system<'ctx, D>(
    context: &mut Context<'ctx, D>,
    function: FunctionDeclaration<'ctx>,
    address: inkwell::values::IntValue<'ctx>,
    abi_data: inkwell::values::BasicValueEnum<'ctx>,
    output_offset: inkwell::values::IntValue<'ctx>,
    output_length: inkwell::values::IntValue<'ctx>,
    extra_abi_data: Vec<inkwell::values::IntValue<'ctx>>,
) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
where
    D: Dependency + Clone,
{
    let status_code_result_pointer = context.build_alloca(
        context.field_type(),
        "system_far_call_result_status_code_pointer",
    );
    context.build_store(status_code_result_pointer, context.field_const(0));

    let far_call_result = context
        .build_call(
            function,
            crate::eravm::utils::external_call_arguments(
                context,
                abi_data,
                address,
                extra_abi_data,
                None,
            )
            .as_slice(),
            "system_far_call_external",
        )
        .expect("IntrinsicFunction always returns a flag");

    let result_abi_data = context
        .builder()
        .build_extract_value(
            far_call_result.into_struct_value(),
            0,
            "system_far_call_external_result_abi_data",
        )
        .expect("Always exists");
    let result_abi_data_pointer = Pointer::new(
        context.byte_type(),
        AddressSpace::Generic,
        result_abi_data.into_pointer_value(),
    );

    let result_status_code_boolean = context
        .builder()
        .build_extract_value(
            far_call_result.into_struct_value(),
            1,
            "system_far_call_external_result_status_code_boolean",
        )
        .expect("Always exists");
    let result_status_code = context.builder().build_int_z_extend_or_bit_cast(
        result_status_code_boolean.into_int_value(),
        context.field_type(),
        "system_far_call_external_result_status_code",
    )?;
    context.build_store(status_code_result_pointer, result_status_code);

    let source = result_abi_data_pointer;

    let destination = Pointer::new_with_offset(
        context,
        AddressSpace::Heap,
        context.byte_type(),
        output_offset,
        "system_far_call_destination",
    );

    context.build_memcpy_return_data(
        context.intrinsics().memory_copy_from_generic,
        destination,
        source,
        output_length,
        "system_far_call_memcpy_from_child",
    )?;

    context.write_abi_pointer(
        result_abi_data_pointer,
        crate::eravm::GLOBAL_RETURN_DATA_POINTER,
    );
    context.write_abi_data_size(
        result_abi_data_pointer,
        crate::eravm::GLOBAL_RETURN_DATA_SIZE,
    );

    let status_code_result =
        context.build_load(status_code_result_pointer, "system_call_status_code");
    Ok(status_code_result)
}

///
/// Checks if the instruction was called with a correct call type.
///
pub fn validate_call_type<'ctx>(
    expected: FunctionDeclaration<'ctx>,
    found: FunctionDeclaration<'ctx>,
    instruction_name: &'static str,
) -> anyhow::Result<()> {
    if expected != found {
        anyhow::bail!(
            "Only `{}` is allowed for the `{}` simulation, found `{}`",
            expected.value.get_name().to_string_lossy(),
            instruction_name,
            found.value.get_name().to_string_lossy()
        );
    }

    Ok(())
}
