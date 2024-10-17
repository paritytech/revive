use inkwell::{
    builder::Builder,
    context::Context,
    module::Module,
    types::{BasicType, StructType},
    values::{BasicValueEnum, PointerValue},
};

/// Creates a module that sets the PolkaVM minimum stack size to [`size`] if linked in.
pub fn min_stack_size<'context>(
    context: &'context Context,
    module_name: &str,
    size: u32,
) -> Module<'context> {
    let module = context.create_module(module_name);
    module.set_inline_assembly(&format!(
        ".pushsection .polkavm_min_stack_size,\"\",@progbits
        .word {size}
        .popsection"
    ));
    module
}

/// Helper for building function calls with stack spilled arguments.
///   - `pointer`: points to a struct of the packed argument struct type
///   - `type`: the packed argument struct type
///   - `arguments`: a correctly ordered list of the struct field values
pub fn spill<'ctx>(
    builder: &Builder<'ctx>,
    pointer: PointerValue<'ctx>,
    r#type: StructType<'ctx>,
    arguments: &[BasicValueEnum<'ctx>],
) -> anyhow::Result<()> {
    for index in 0..r#type.get_field_types().len() {
        let field_pointer = builder.build_struct_gep(
            r#type,
            pointer,
            index as u32,
            &format!("spill_parameter_{}", index),
        )?;
        let field_value = arguments
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("invalid index {index} for struct type {}", r#type))?;
        builder.build_store(field_pointer, *field_value)?;
    }

    Ok(())
}

/// Returns a packed struct argument type for the `instantiate` API.
pub fn instantiate(context: &Context) -> StructType {
    context.struct_type(
        &[
            // code_hash_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // ref_time_limit: u64,
            context.i64_type().as_basic_type_enum(),
            // proof_size_limit: u64,
            context.i64_type().as_basic_type_enum(),
            // deposit_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // value_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // input_data_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // input_data_len: u32,
            context.i32_type().as_basic_type_enum(),
            // address_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // output_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // output_len_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // salt_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
        ],
        true,
    )
}

/// Returns a packed struct argument type for the `call` API.
pub fn call(context: &Context) -> StructType {
    context.struct_type(
        &[
            // flags: u32,
            context.i32_type().as_basic_type_enum(),
            // address_ptr:
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // ref_time_limit: u64,
            context.i64_type().as_basic_type_enum(),
            // proof_size_limit: u64,
            context.i64_type().as_basic_type_enum(),
            // deposit_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // value_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // input_data_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // input_data_len: u32,
            context.i32_type().as_basic_type_enum(),
            // output_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // output_len_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
        ],
        true,
    )
}

/// Returns a packed struct argument type for the `delegate_call` API.
pub fn delegate_call(context: &Context) -> StructType {
    context.struct_type(
        &[
            // flags: u32,
            context.i32_type().as_basic_type_enum(),
            // address_ptr:
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // ref_time_limit: u64,
            context.i64_type().as_basic_type_enum(),
            // proof_size_limit: u64,
            context.i64_type().as_basic_type_enum(),
            // input_data_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // input_data_len: u32,
            context.i32_type().as_basic_type_enum(),
            // output_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
            // output_len_ptr: u32,
            context.ptr_type(Default::default()).as_basic_type_enum(),
        ],
        true,
    )
}
