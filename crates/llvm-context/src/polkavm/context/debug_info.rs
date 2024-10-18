//! The LLVM debug information.

use std::cell::RefCell;

use inkwell::debug_info::AsDIScope;
use inkwell::debug_info::DIScope;

/// Debug info scope stack
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeStack<'ctx> {
    stack: Vec<DIScope<'ctx>>,
}

// Abstract the type of the DIScope stack.
impl<'ctx> ScopeStack<'ctx> {
    pub fn from(item: DIScope<'ctx>) -> Self {
        Self { stack: vec![item] }
    }

    /// Return the top of the scope stack, or None if the stack is empty.
    pub fn top(&self) -> Option<DIScope<'ctx>> {
        self.stack.last().copied()
    }

    /// Push a scope onto the stack.
    pub fn push(&mut self, scope: DIScope<'ctx>) {
        self.stack.push(scope)
    }

    /// Pop the scope at the top of the stack and return it.
    /// Return None if the stack is empty.
    pub fn pop(&mut self) -> Option<DIScope<'ctx>> {
        self.stack.pop()
    }

    /// Return the number of scopes on the stack.
    pub fn len(&self) -> usize {
        self.stack.len()
    }
}

/// The LLVM debug information.
pub struct DebugInfo<'ctx> {
    /// The compile unit.
    compile_unit: inkwell::debug_info::DICompileUnit<'ctx>,
    /// The debug info builder.
    builder: inkwell::debug_info::DebugInfoBuilder<'ctx>,
    /// Enclosing debug info scopes.
    scope_stack: RefCell<ScopeStack<'ctx>>,
    // Names of enclosing objects, functions and other namespaces.
    namespace_stack: RefCell<Vec<String>>,
}

impl<'ctx> DebugInfo<'ctx> {
    /// A shortcut constructor.
    pub fn new(module: &inkwell::module::Module<'ctx>) -> Self {
        let (builder, compile_unit) = module.create_debug_info_builder(
            true,
            inkwell::debug_info::DWARFSourceLanguage::C,
            module.get_name().to_string_lossy().as_ref(),
            "",
            "",
            false,
            "",
            0,
            "",
            inkwell::debug_info::DWARFEmissionKind::Full,
            0,
            false,
            false,
            "",
            "",
        );

        Self {
            compile_unit,
            builder,
            scope_stack: RefCell::new(ScopeStack::from(compile_unit.as_debug_info_scope())),
            namespace_stack: RefCell::new(vec![]),
        }
    }

    /// Prepare an LLVM-IR module for debug-info generation
    pub fn initialize_module(
        &self,
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
    ) {
        let debug_metadata_value = llvm
            .i32_type()
            .const_int(inkwell::debug_info::debug_metadata_version() as u64, false);
        module.add_basic_value_flag(
            "Debug Info Version",
            inkwell::module::FlagBehavior::Warning,
            debug_metadata_value,
        );
        self.push_scope(self.compilation_unit().get_file().as_debug_info_scope());
    }

    /// Finalize debug-info for an LLVM-IR module.
    pub fn finalize_module(&self) {
        self.builder().finalize()
    }

    /// Creates a function info.
    pub fn create_function(
        &self,
        name: &str,
    ) -> anyhow::Result<inkwell::debug_info::DISubprogram<'ctx>> {
        let flags = inkwell::debug_info::DIFlagsConstants::ZERO;
        let subroutine_type = self.builder.create_subroutine_type(
            self.compile_unit.get_file(),
            Some(self.create_word_type(Some(flags))?.as_type()),
            &[],
            flags,
        );

        let function = self.builder.create_function(
            self.compile_unit.get_file().as_debug_info_scope(),
            name,
            None,
            self.compile_unit.get_file(),
            42,
            subroutine_type,
            true,
            false,
            1,
            flags,
            false,
        );

        self.builder.create_lexical_block(
            function.as_debug_info_scope(),
            self.compile_unit.get_file(),
            1,
            1,
        );

        Ok(function)
    }

    /// Creates primitive integer type debug-info.
    pub fn create_primitive_type(
        &self,
        bit_length: usize,
        flags: Option<inkwell::debug_info::DIFlags>,
    ) -> anyhow::Result<inkwell::debug_info::DIBasicType<'ctx>> {
        let di_flags = flags.unwrap_or(inkwell::debug_info::DIFlagsConstants::ZERO);
        let di_encoding: u32 = 0;
        let type_name = String::from("U") + bit_length.to_string().as_str();
        self.builder
            .create_basic_type(type_name.as_str(), bit_length as u64, di_encoding, di_flags)
            .map_err(|error| anyhow::anyhow!("Debug info error: {}", error))
    }

    /// Returns the debug-info model of word-sized integer types.
    pub fn create_word_type(
        &self,
        flags: Option<inkwell::debug_info::DIFlags>,
    ) -> anyhow::Result<inkwell::debug_info::DIBasicType<'ctx>> {
        self.create_primitive_type(revive_common::BIT_LENGTH_WORD, flags)
    }

    /// Return the DIBuilder.
    pub fn builder(&self) -> &inkwell::debug_info::DebugInfoBuilder<'ctx> {
        &self.builder
    }

    /// Return the compilation unit. {
    pub fn compilation_unit(&self) -> &inkwell::debug_info::DICompileUnit<'ctx> {
        &self.compile_unit
    }

    /// Push a debug-info scope onto the stack.
    pub fn push_scope(&self, scope: DIScope<'ctx>) {
        self.scope_stack.borrow_mut().push(scope)
    }

    /// Pop the top of the debug-info scope stack and return it.
    pub fn pop_scope(&self) -> Option<DIScope<'ctx>> {
        self.scope_stack.borrow_mut().pop()
    }

    /// Return the top of the debug-info scope stack.
    pub fn top_scope(&self) -> Option<DIScope<'ctx>> {
        self.scope_stack.borrow().top()
    }

    /// Return the number of debug-info scopes on the scope stack.
    pub fn num_scopes(&self) -> usize {
        self.scope_stack.borrow().len()
    }

    /// Push a name onto the namespace stack.
    pub fn push_namespace(&self, name: String) {
        self.namespace_stack.borrow_mut().push(name);
    }

    /// Pop the top name off the namespace stack and return it.
    pub fn pop_namespace(&self) -> Option<String> {
        self.namespace_stack.borrow_mut().pop()
    }

    /// Return the top of the namespace stack.
    pub fn top_namespace(&self) -> Option<String> {
        self.namespace_stack.borrow().last().cloned()
    }

    // Get a string representation of the namespace stack. Optionally append the given name.
    pub fn namespace_as_identifier(&self, name: Option<&str>) -> String {
        let separator = "::";
        let mut ret = String::new();
        let mut sep = false;
        for s in self.namespace_stack.borrow().iter() {
            if sep {
                ret.push_str(separator);
            };
            sep = true;
            ret.push_str(s)
        }
        if let Some(n) = name {
            if sep {
                ret.push_str(separator);
            };
            ret.push_str(n);
        }
        ret
    }
}
