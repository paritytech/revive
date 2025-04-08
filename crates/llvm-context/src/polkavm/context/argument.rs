//! The LLVM argument with metadata.

/// The LLVM argument with metadata.
#[derive(Debug, Clone)]
pub struct Argument<'ctx> {
    /// The actual LLVM operand.
    pub value: Value<'ctx>,
    /// The original AST value. Used mostly for string literals.
    pub original: Option<String>,
    /// The preserved constant value, if available.
    pub constant: Option<num::BigUint>,
}

/// The function argument can be either a pointer or a integer value.
/// This disambiguation allows for lazy loading of variables.
#[derive(Clone, Debug)]
pub enum Value<'ctx> {
    Register(inkwell::values::BasicValueEnum<'ctx>),
    Pointer {
        pointer: crate::polkavm::context::Pointer<'ctx>,
        id: String,
    },
}

impl<'ctx> Argument<'ctx> {
    /// A shortcut constructor for register arguments.
    pub fn value(value: inkwell::values::BasicValueEnum<'ctx>) -> Self {
        Self {
            value: Value::Register(value),
            original: None,
            constant: None,
        }
    }

    /// A shortcut constructor for stack arguments.
    pub fn pointer(pointer: crate::polkavm::context::Pointer<'ctx>, id: String) -> Self {
        Self {
            value: Value::Pointer { pointer, id },
            original: None,
            constant: None,
        }
    }

    /// Set the original decleratation value.
    pub fn with_original(mut self, original: String) -> Self {
        self.original = Some(original);
        self
    }

    /// Set the constant value.
    pub fn with_constant(mut self, constant: num::BigUint) -> Self {
        self.constant = Some(constant);
        self
    }

    /// Returns the inner LLVM value.
    ///
    /// Panics if `self` is a pointer argument.
    pub fn _to_llvm_value(&self) -> inkwell::values::BasicValueEnum<'ctx> {
        match &self.value {
            Value::Register(value) => *value,
            Value::Pointer { .. } => unreachable!("invalid register value access"),
        }
    }

    /// Access the underlying value.
    ///
    /// Will emit a stack load if `self` is a pointer argument.
    pub fn access<D: crate::polkavm::Dependency + Clone>(
        &self,
        context: &crate::polkavm::context::Context<'ctx, D>,
    ) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
        match &self.value {
            Value::Register(value) => Ok(*value),
            Value::Pointer { pointer, id } => context.build_load(*pointer, id),
        }
    }

    /// Access the underlying value.
    ///
    /// Will emit a stack load if `self` is a pointer argument.
    pub fn as_pointer<D: crate::polkavm::Dependency + Clone>(
        &self,
        context: &crate::polkavm::context::Context<'ctx, D>,
    ) -> anyhow::Result<crate::polkavm::context::Pointer<'ctx>> {
        match &self.value {
            Value::Register(value) => {
                let pointer = context.build_alloca_at_entry(context.word_type(), "pvm_arg");
                context.build_store(pointer, *value)?;
                Ok(pointer)
            }
            Value::Pointer { pointer, .. } => Ok(*pointer),
        }
    }
}

impl<'ctx> From<inkwell::values::BasicValueEnum<'ctx>> for Argument<'ctx> {
    fn from(value: inkwell::values::BasicValueEnum<'ctx>) -> Self {
        Self::value(value)
    }
}
