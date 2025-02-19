//! The front-end runtime functions.

use crate::polkavm::context::function::Function;
use crate::polkavm::context::Context;
use crate::polkavm::Dependency;

pub mod deploy_code;
pub mod entry;
pub mod immutable_data_load;
pub mod runtime_code;

/// The main entry function name.
pub const FUNCTION_ENTRY: &str = "__entry";

/// The deploy code function name.
pub const FUNCTION_DEPLOY_CODE: &str = "__deploy";

/// The runtime code function name.
pub const FUNCTION_RUNTIME_CODE: &str = "__runtime";

/// The immutable data load function name.
pub const FUNCTION_LOAD_IMMUTABLE_DATA: &str = "__immutable_data_load";

pub trait RuntimeFunction<D>
where
    D: Dependency + Clone,
{
    /// The function name.
    const FUNCTION_NAME: &'static str;

    /// The function attributes.
    const FUNCTION_ATTRIBUTES: &'static [crate::polkavm::context::function::Attribute];

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
}
