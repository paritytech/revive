use inkwell::{builder::Builder, context::Context, module::Module, values::IntValue};

/// Creates a module that sets the PolkaVM minimum stack size to `size` if linked in.
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

/// Helper for packing two 32 bit integer values into a 64 bit integer value.
pub fn pack_hi_lo_reg<'ctx>(
    builder: &Builder<'ctx>,
    context: &'ctx Context,
    hi: IntValue<'ctx>,
    lo: IntValue<'ctx>,
    name: &str,
) -> anyhow::Result<IntValue<'ctx>> {
    assert_eq!(hi.get_type(), context.i32_type());
    assert_eq!(lo.get_type(), context.i32_type());

    let lo_part = builder.build_int_z_extend(lo, context.i64_type(), &format!("{name}_lo_part"))?;
    let hi_part = builder.build_int_z_extend(hi, context.i64_type(), &format!("{name}_hi_part"))?;
    let hi_part_shifted = builder.build_left_shift(
        hi_part,
        context.i64_type().const_int(32, false),
        &format!("{name}_hi_part_shifted"),
    )?;
    Ok(builder.build_or(hi_part_shifted, lo_part, name)?)
}
