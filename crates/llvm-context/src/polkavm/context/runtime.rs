//! The revive compiler runtime function interface definition.
//!
//! Common routines are not implicitly inlined but extracted into smaller functions.
//! This benefits contract code size.

use crate::polkavm::context::function::declaration::Declaration;
use crate::polkavm::context::function::Function;
use crate::polkavm::context::Attribute;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

/// The revive runtime function interface simplifies declaring runtime functions
/// and code emitting by providing helpful default implementations.
pub trait RuntimeFunction<D>
where
    D: Dependency + Clone,
{
    /// The function name.
    const NAME: &'static str;

    const ATTRIBUTES: &'static [Attribute] = &[Attribute::NoFree, Attribute::WillReturn];

    /// The function type.
    fn r#type<'ctx>(context: &Context<'ctx, D>) -> inkwell::types::FunctionType<'ctx>;

    /// Declare the function.
    fn declare(&self, context: &mut Context<D>) -> anyhow::Result<()> {
        let function = context.add_function(
            Self::NAME,
            Self::r#type(context),
            0,
            Some(inkwell::module::Linkage::External),
        )?;
        Function::set_attributes(
            context.llvm(),
            function.borrow().declaration(),
            Self::ATTRIBUTES,
            true,
        );
        Ok(())
    }

    fn declaration<'ctx>(context: &Context<'ctx, D>) -> Declaration<'ctx> {
        context
            .get_function(Self::NAME)
            .unwrap_or_else(|| panic!("runtime function {} should have been declared", Self::NAME))
            .borrow()
            .declaration()
    }

    /// Emit the function.
    fn emit(&self, context: &mut Context<D>) -> anyhow::Result<()> {
        context.set_current_function(Self::NAME, None)?;
        context.set_basic_block(context.current_function().borrow().entry_block());

        let return_value = self.emit_body(context)?;
        self.emit_epilogue(context, return_value);

        context.pop_debug_scope();

        Ok(())
    }

    /// Emit the function body.
    fn emit_body<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>>;

    /// Emit the function return instructions.
    fn emit_epilogue<'ctx>(
        &self,
        context: &mut Context<'ctx, D>,
        return_value: Option<inkwell::values::BasicValueEnum<'ctx>>,
    ) {
        let return_block = context.current_function().borrow().return_block();
        context.build_unconditional_branch(return_block);
        context.set_basic_block(return_block);
        match return_value {
            Some(value) => context.build_return(Some(&value)),
            None => context.build_return(None),
        }
    }

    /// Get the nth function paramater.
    fn paramater<'ctx>(
        context: &Context<'ctx, D>,
        nth: u32,
    ) -> inkwell::values::BasicValueEnum<'ctx> {
        let name = Self::NAME;
        context
            .get_function(name)
            .unwrap_or_else(|| panic!("runtime function {name} should have been declared"))
            .borrow()
            .declaration()
            .function_value()
            .get_nth_param(nth)
            .unwrap_or_else(|| panic!("runtime function {name} should have parameter {nth}"))
    }
}
