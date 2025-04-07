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

#[derive(Clone, Debug)]
pub enum Value<'ctx> {
    Register(inkwell::values::BasicValueEnum<'ctx>),
    Pointer(inkwell::values::PointerValue<'ctx>),
}

impl<'ctx> Argument<'ctx> {
    /// The calldata offset argument index.
    pub const ARGUMENT_INDEX_CALLDATA_OFFSET: usize = 0;

    /// The calldata length argument index.
    pub const ARGUMENT_INDEX_CALLDATA_LENGTH: usize = 1;

    /// A shortcut constructor.
    pub fn new_value(value: inkwell::values::BasicValueEnum<'ctx>) -> Self {
        Self {
            value: Value::Register(value),
            original: None,
            constant: None,
        }
    }

    /// A shortcut constructor.
    pub fn new_with_original(
        value: inkwell::values::BasicValueEnum<'ctx>,
        original: String,
    ) -> Self {
        Self {
            value: Value::Register(value),
            original: Some(original),
            constant: None,
        }
    }

    /// A shortcut constructor.
    pub fn new_with_constant(
        value: inkwell::values::BasicValueEnum<'ctx>,
        constant: num::BigUint,
    ) -> Self {
        Self {
            value: Value::Register(value),
            original: None,
            constant: Some(constant),
        }
    }

    /// Returns the inner LLVM value.
    pub fn to_llvm_value(&self) -> inkwell::values::BasicValueEnum<'ctx> {
        match self.value {
            Value::Register(value) => value,
            Value::Pointer(_ptr) => todo!(),
        }
    }
}

impl<'ctx> From<inkwell::values::BasicValueEnum<'ctx>> for Argument<'ctx> {
    fn from(value: inkwell::values::BasicValueEnum<'ctx>) -> Self {
        Self::new_value(value)
    }
}
