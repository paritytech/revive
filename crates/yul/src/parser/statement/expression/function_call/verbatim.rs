//! Translates the verbatim simulations.

use crate::parser::statement::expression::function_call::FunctionCall;

/// Translates the verbatim simulations.
pub fn verbatim<'ctx, D>(
    context: &mut revive_llvm_context::PolkaVMContext<'ctx, D>,
    call: &mut FunctionCall,
    _input_size: usize,
    output_size: usize,
) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>>
where
    D: revive_llvm_context::PolkaVMDependency + Clone,
{
    if output_size > 1 {
        anyhow::bail!(
            "{} Verbatim instructions with multiple return values are not supported",
            call.location
        );
    }

    let mut arguments = call.pop_arguments::<D, 1>(context)?;
    let identifier = arguments[0]
        .original
        .take()
        .ok_or_else(|| anyhow::anyhow!("{} Verbatim literal is missing", call.location))?;

    anyhow::bail!(
        "{} Found unknown internal function `{}`",
        call.location,
        identifier
    )
}
