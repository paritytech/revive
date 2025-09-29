//! The revive compiler runtime function interface definition.
//!
//! Common routines should not be inlined but extracted into smaller functions.
//! This benefits contract code size.

use crate::optimizer::settings::size_level::SizeLevel;
use crate::polkavm::context::function::declaration::Declaration;
use crate::polkavm::context::function::Function;
use crate::polkavm::context::Attribute;
use crate::polkavm::context::Context;

/// The revive runtime function interface simplifies declaring runtime functions
/// and code emitting by providing helpful default implementations.
pub trait RuntimeFunction {
    /// The function name.
    const NAME: &'static str;

    const ATTRIBUTES: &'static [Attribute] = &[
        Attribute::NoFree,
        Attribute::NoRecurse,
        Attribute::WillReturn,
    ];

    /// The function type.
    fn r#type<'ctx>(context: &Context<'ctx>) -> inkwell::types::FunctionType<'ctx>;

    /// Declare the function.
    fn declare(&self, context: &mut Context) -> anyhow::Result<()> {
        let function = context.add_function(
            Self::NAME,
            Self::r#type(context),
            0,
            Some(inkwell::module::Linkage::Private),
            None,
        )?;

        let mut attributes = Self::ATTRIBUTES.to_vec();
        attributes.extend_from_slice(match context.optimizer_settings().level_middle_end_size {
            SizeLevel::Zero => &[],
            _ => &[Attribute::OptimizeForSize, Attribute::MinSize],
        });
        Function::set_attributes(
            context.llvm(),
            function.borrow().declaration(),
            &attributes,
            true,
        );

        Ok(())
    }

    /// Get the function declaration.
    fn declaration<'ctx>(context: &Context<'ctx>) -> Declaration<'ctx> {
        context
            .get_function(Self::NAME)
            .unwrap_or_else(|| panic!("runtime function {} should be declared", Self::NAME))
            .borrow()
            .declaration()
    }

    /// Emit the function.
    fn emit(&self, context: &mut Context) -> anyhow::Result<()> {
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
        context: &mut Context<'ctx>,
    ) -> anyhow::Result<Option<inkwell::values::BasicValueEnum<'ctx>>>;

    /// Emit the function return instructions.
    fn emit_epilogue<'ctx>(
        &self,
        context: &mut Context<'ctx>,
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
        context: &Context<'ctx>,
        index: usize,
    ) -> inkwell::values::BasicValueEnum<'ctx> {
        let name = Self::NAME;
        context
            .get_function(name)
            .unwrap_or_else(|| panic!("runtime function {name} should be declared"))
            .borrow()
            .declaration()
            .function_value()
            .get_nth_param(index as u32)
            .unwrap_or_else(|| panic!("runtime function {name} should have parameter #{index}"))
    }
}
