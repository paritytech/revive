//! The revive compiler runtime function interface definition.
//!
//! Common routines are not implicitly inlined but extracted into smaller functions.
//! This benefits contract code size.

use crate::polkavm::context::function::Function;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// The revive runtime function interface simplifies declaring runtime functions
/// and code emitting by providing helpful default implementations.
pub trait RuntimeFunction<D>
where
    D: Dependency + Clone,
{
    /// The function name.
    const FUNCTION_NAME: &'static str;

    /// The function attributes.
    const FUNCTION_ATTRIBUTES: &'static [crate::polkavm::context::Attribute];

    /// The function type.
    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx>;

    /// Declare the function.
    fn declare(&self, context: &mut Context<D>) -> anyhow::Result<()> {
        let function = context.add_function(
            Self::FUNCTION_NAME,
            Self::r#type(context),
            0,
            Some(inkwell::module::Linkage::External),
        )?;
        Function::set_attributes(
            context.llvm(),
            function.borrow().declaration(),
            Self::FUNCTION_ATTRIBUTES,
            true,
        );
        Ok(())
    }

    /// Emit the function.
    fn emit(&self, context: &mut Context<D>) -> anyhow::Result<()> {
        context.set_current_function(Self::FUNCTION_NAME, None)?;
        context.set_basic_block(context.current_function().borrow().entry_block());

        self.emit_body(context)?;

        context.pop_debug_scope();

        Ok(())
    }

    /// Emit the function body.
    fn emit_body(&self, context: &Context<D>) -> anyhow::Result<()>;

    /// Get the nth function paramater.
    fn paramater<'ctx>(
        context: &Context<'ctx, D>,
        nth: u32,
    ) -> inkwell::values::BasicValueEnum<'ctx> {
        let name = Self::FUNCTION_NAME;
        context
            .get_function(name)
            .unwrap_or_else(|| panic!("runtime function {name} should have been declared",))
            .borrow()
            .declaration()
            .function_value()
            .get_nth_param(nth)
            .unwrap_or_else(|| panic!("runtime function {name} should have parameter {nth}",))
    }
}
