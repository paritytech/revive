//! The LLVM IR generator context.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use inkwell::debug_info::AsDIScope;
use inkwell::debug_info::DIScope;
use inkwell::types::BasicType;
use inkwell::values::BasicValue;
use inkwell::values::InstructionOpcode;
use revive_solc_json_interface::PolkaVMDefaultHeapMemorySize;
use revive_solc_json_interface::PolkaVMDefaultStackMemorySize;
use revive_solc_json_interface::SolcStandardJsonInputSettingsPolkaVMMemory;

use crate::optimizer::settings::Settings as OptimizerSettings;
use crate::optimizer::Optimizer;
use crate::polkavm::DebugConfig;
use crate::target_machine::target::Target;
use crate::target_machine::TargetMachine;
use crate::PolkaVMLoadHeapWordFunction;
use crate::PolkaVMLoadHeapWordNativeFunction;
use crate::PolkaVMSbrkFunction;
use crate::PolkaVMStoreHeapWordFunction;
use crate::PolkaVMStoreHeapWordNativeFunction;

use self::address_space::AddressSpace;
use self::attribute::Attribute;
use self::build::Build;
use self::code_type::CodeType;
use self::debug_info::DebugInfo;
use self::function::declaration::Declaration as FunctionDeclaration;
use self::function::intrinsics::Intrinsics;
use self::function::llvm_runtime::LLVMRuntime;
use self::function::r#return::Return as FunctionReturn;
use self::function::runtime::revive::Exit;
use self::function::runtime::revive::WordToPointer;
use self::function::Function;
use self::global::Global;
use self::pointer::Pointer;
use self::r#loop::Loop;
use self::runtime::RuntimeFunction;
use self::solidity_data::SolidityData;
use self::yul_data::YulData;

pub mod address_space;
pub mod argument;
pub mod attribute;
pub mod build;
pub mod code_type;
pub mod debug_info;
pub mod function;
pub mod global;
pub mod r#loop;
pub mod pointer;
pub mod runtime;
pub mod solidity_data;
pub mod yul_data;

#[cfg(test)]
mod tests;

/// The LLVM IR generator context.
/// It is a not-so-big god-like object glueing all the compilers' complexity and act as an adapter
/// and a superstructure over the inner `inkwell` LLVM context.
pub struct Context<'ctx> {
    /// The inner LLVM context.
    llvm: &'ctx inkwell::context::Context,
    /// The inner LLVM context builder.
    builder: inkwell::builder::Builder<'ctx>,
    /// The optimization tools.
    optimizer: Optimizer,
    /// The current module.
    module: inkwell::module::Module<'ctx>,
    /// The current contract code type, which can be deploy or runtime.
    code_type: Option<CodeType>,
    /// The global variables.
    globals: HashMap<String, Global<'ctx>>,
    /// The LLVM intrinsic functions, defined on the LLVM side.
    intrinsics: Intrinsics<'ctx>,
    /// The LLVM runtime functions, defined on the LLVM side.
    llvm_runtime: LLVMRuntime<'ctx>,
    /// The declared functions.
    functions: HashMap<String, Rc<RefCell<Function<'ctx>>>>,
    /// The current active function.
    current_function: Option<Rc<RefCell<Function<'ctx>>>>,
    /// The loop context stack.
    loop_stack: Vec<Loop<'ctx>>,
    /// The PVM memory configuration.
    memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,

    /// The debug info of the current module.
    debug_info: Option<DebugInfo<'ctx>>,
    /// The debug configuration telling whether to dump the needed IRs.
    debug_config: DebugConfig,

    /// The Solidity data.
    solidity_data: Option<SolidityData>,
    /// The Yul data.
    yul_data: Option<YulData>,
}

impl<'ctx> Context<'ctx> {
    /// The functions hashmap default capacity.
    const FUNCTIONS_HASHMAP_INITIAL_CAPACITY: usize = 64;

    /// The globals hashmap default capacity.
    const GLOBALS_HASHMAP_INITIAL_CAPACITY: usize = 4;

    /// The loop stack default capacity.
    const LOOP_STACK_INITIAL_CAPACITY: usize = 16;

    /// Link in the stdlib module.
    fn link_stdlib_module(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
    ) {
        module
            .link_in_module(revive_stdlib::module(llvm, "revive_stdlib").unwrap())
            .expect("the stdlib module should be linkable");
    }

    /// Link in the PolkaVM imports module, containing imported functions,
    /// and marking them as external (they need to be relocatable as too).
    fn link_polkavm_imports(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
    ) {
        module
            .link_in_module(
                revive_runtime_api::polkavm_imports::module(llvm, "polkavm_imports").unwrap(),
            )
            .expect("the PolkaVM imports module should be linkable");

        for import in revive_runtime_api::polkavm_imports::IMPORTS {
            module
                .get_function(import)
                .unwrap_or_else(|| panic!("{import} import should be declared"))
                .set_linkage(inkwell::module::Linkage::Internal);
        }
    }

    fn link_polkavm_exports(&self, contract_path: &str) -> anyhow::Result<()> {
        let exports = revive_runtime_api::polkavm_exports::module(self.llvm(), "polkavm_exports")
            .map_err(|error| {
            anyhow::anyhow!(
                "The contract `{}` exports module loading error: {}",
                contract_path,
                error
            )
        })?;
        self.module.link_in_module(exports).map_err(|error| {
            anyhow::anyhow!(
                "The contract `{}` exports module linking error: {}",
                contract_path,
                error
            )
        })
    }

    fn link_immutable_data(&self, contract_path: &str) -> anyhow::Result<()> {
        let size = self.solidity().immutables_size() as u32;
        let immutables = revive_runtime_api::immutable_data::module(self.llvm(), size);

        self.module.link_in_module(immutables).map_err(|error| {
            anyhow::anyhow!(
                "The contract `{}` immutable data module linking error: {}",
                contract_path,
                error
            )
        })
    }

    /// Configure the PolkaVM minimum stack size.
    fn set_polkavm_stack_size(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
        size: u32,
    ) {
        module
            .link_in_module(revive_runtime_api::calling_convention::min_stack_size(
                llvm,
                "polkavm_stack_size",
                size,
            ))
            .expect("the PolkaVM minimum stack size module should be linkable");
    }

    /// PolkaVM wants PIE code; we set this flag on the module here.
    fn set_module_flags(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
    ) {
        module.add_basic_value_flag(
            "PIE Level",
            inkwell::module::FlagBehavior::Override,
            llvm.i32_type().const_int(2, false),
        );
    }

    /// Configure the revive datalayout.
    fn set_data_layout(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
    ) {
        let source_module = revive_stdlib::module(llvm, "revive_stdlib").unwrap();
        let data_layout = source_module.get_data_layout();
        module.set_data_layout(&data_layout);
    }

    /// Initializes a new LLVM context.
    pub fn new(
        llvm: &'ctx inkwell::context::Context,
        module: inkwell::module::Module<'ctx>,
        optimizer: Optimizer,
        debug_config: DebugConfig,
        memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
    ) -> Self {
        Self::set_data_layout(llvm, &module);
        Self::link_stdlib_module(llvm, &module);
        Self::link_polkavm_imports(llvm, &module);
        Self::set_polkavm_stack_size(
            llvm,
            &module,
            memory_config
                .stack_size
                .unwrap_or(PolkaVMDefaultStackMemorySize),
        );
        Self::set_module_flags(llvm, &module);

        let intrinsics = Intrinsics::new(llvm, &module);
        let llvm_runtime = LLVMRuntime::new(llvm, &module, &optimizer);
        let debug_info = debug_config.emit_debug_info.then(|| {
            let debug_info = DebugInfo::new(&module, &debug_config);
            debug_info.initialize_module(llvm, &module);
            debug_info
        });

        Self {
            llvm,
            builder: llvm.create_builder(),
            optimizer,
            module,
            code_type: None,
            globals: HashMap::with_capacity(Self::GLOBALS_HASHMAP_INITIAL_CAPACITY),
            intrinsics,
            llvm_runtime,
            functions: HashMap::with_capacity(Self::FUNCTIONS_HASHMAP_INITIAL_CAPACITY),
            current_function: None,
            loop_stack: Vec::with_capacity(Self::LOOP_STACK_INITIAL_CAPACITY),
            memory_config,

            debug_info,
            debug_config,

            solidity_data: None,
            yul_data: None,
        }
    }

    /// Initializes a new dummy LLVM context.
    ///
    /// Omits the LLVM module initialization; use this only in tests and benchmarks.
    pub fn new_dummy(
        llvm: &'ctx inkwell::context::Context,
        optimizer_settings: OptimizerSettings,
    ) -> Self {
        Self::new(
            llvm,
            llvm.create_module("dummy"),
            Optimizer::new(optimizer_settings),
            Default::default(),
            Default::default(),
        )
    }

    /// Builds the LLVM IR module, returning the build artifacts.
    pub fn build(
        self,
        contract_path: &str,
        metadata_hash: Option<revive_common::Keccak256>,
    ) -> anyhow::Result<Build> {
        self.link_polkavm_exports(contract_path)?;
        self.link_immutable_data(contract_path)?;

        let target_machine = TargetMachine::new(Target::PVM, self.optimizer.settings())?;
        self.module().set_triple(&target_machine.get_triple());

        self.debug_config
            .dump_llvm_ir_unoptimized(contract_path, self.module())?;

        self.verify().map_err(|error| {
            anyhow::anyhow!(
                "The contract `{}` unoptimized LLVM IR verification error: {}",
                contract_path,
                error
            )
        })?;

        self.optimizer
            .run(&target_machine, self.module())
            .map_err(|error| {
                anyhow::anyhow!(
                    "The contract `{}` optimizing error: {}",
                    contract_path,
                    error
                )
            })?;

        // Narrow large integer div/rem where operands provably fit in a smaller
        // type. LLVM's DivRemNarrowing pass sometimes fails on large functions,
        // leaving behind i256 div/rem that triggers strip_minsize_for_divrem.
        self.narrow_divrem_instructions();

        // Remove MinSize on functions that perform large integer div/rem to
        // avoid compiler crash that happens when large integer div/rem by
        // power-of-2 are not being expanded by ExpandLargeIntDivRem pass as
        // it expects peephole from DAGCombine, which doesn't happen due to the
        // MinSize attribute being set on the function.
        // NOTE: As soon as it strips attribute from a function where large
        // integer div/rem is used, it's crucial to call it after inlining.
        // TODO: Remove this once LLVM fix is backported to LLVM 21 and we
        // switch to corresponding inkwell version.
        self.strip_minsize_for_divrem();

        self.debug_config
            .dump_llvm_ir_optimized(contract_path, self.module())?;

        self.verify().map_err(|error| {
            anyhow::anyhow!(
                "The contract `{}` optimized LLVM IR verification error: {}",
                contract_path,
                error
            )
        })?;

        let buffer = target_machine
            .write_to_memory_buffer(self.module())
            .map_err(|error| {
                anyhow::anyhow!(
                    "The contract `{}` assembly generating error: {}",
                    contract_path,
                    error
                )
            })?;

        let object = buffer.as_slice().to_vec();

        self.debug_config.dump_object(contract_path, &object)?;

        crate::polkavm::build(
            &object,
            metadata_hash
                .as_ref()
                .map(|hash| hash.as_bytes().try_into().unwrap()),
        )
    }

    /// Verifies the current LLVM IR module.
    pub fn verify(&self) -> anyhow::Result<()> {
        self.module()
            .verify()
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    /// Returns the inner LLVM context.
    pub fn llvm(&self) -> &'ctx inkwell::context::Context {
        self.llvm
    }

    /// Returns the LLVM IR builder.
    pub fn builder(&self) -> &inkwell::builder::Builder<'ctx> {
        &self.builder
    }

    /// Returns the current LLVM IR module reference.
    pub fn module(&self) -> &inkwell::module::Module<'ctx> {
        &self.module
    }

    /// Sets the current code type (deploy or runtime).
    pub fn set_code_type(&mut self, code_type: CodeType) {
        self.code_type = Some(code_type);
    }

    /// Returns the current code type (deploy or runtime).
    pub fn code_type(&self) -> Option<CodeType> {
        self.code_type.to_owned()
    }

    /// Returns the function value of a runtime API method.
    pub fn runtime_api_method(&self, name: &'static str) -> inkwell::values::FunctionValue<'ctx> {
        self.module()
            .get_function(name)
            .unwrap_or_else(|| panic!("runtime API method {name} not declared"))
    }

    /// Returns the pointer to a global variable.
    pub fn get_global(&self, name: &str) -> anyhow::Result<Global<'ctx>> {
        match self.globals.get(name) {
            Some(global) => Ok(*global),
            None => anyhow::bail!("Global variable {} is not declared", name),
        }
    }

    /// Returns the value of a global variable.
    pub fn get_global_value(
        &self,
        name: &str,
    ) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
        let global = self.get_global(name)?;
        self.build_load(global.into(), name)
    }

    /// Sets the value to a global variable.
    pub fn set_global<T, V>(&mut self, name: &str, r#type: T, address_space: AddressSpace, value: V)
    where
        T: BasicType<'ctx> + Clone + Copy,
        V: BasicValue<'ctx> + Clone + Copy,
    {
        match self.globals.get(name) {
            Some(global) => {
                let global = *global;
                self.build_store(global.into(), value).unwrap();
            }
            None => {
                let global = Global::new(self, r#type, address_space, value, name);
                self.globals.insert(name.to_owned(), global);
            }
        }
    }

    /// Declare an external global. This is an idempotent method.
    pub fn declare_global<T>(&mut self, name: &str, r#type: T, address_space: AddressSpace)
    where
        T: BasicType<'ctx> + Clone + Copy,
    {
        if self.globals.contains_key(name) {
            return;
        }

        let global = Global::declare(self, r#type, address_space, name);
        self.globals.insert(name.to_owned(), global);
    }

    /// Returns the LLVM intrinsics collection reference.
    pub fn intrinsics(&self) -> &Intrinsics<'ctx> {
        &self.intrinsics
    }

    /// Returns the LLVM runtime function collection reference.
    pub fn llvm_runtime(&self) -> &LLVMRuntime<'ctx> {
        &self.llvm_runtime
    }

    /// Appends a function to the current module.
    pub fn add_function(
        &mut self,
        name: &str,
        r#type: inkwell::types::FunctionType<'ctx>,
        return_values_length: usize,
        linkage: Option<inkwell::module::Linkage>,
        location: Option<(u32, u32)>,
        is_frontend: bool,
    ) -> anyhow::Result<Rc<RefCell<Function<'ctx>>>> {
        assert!(
            self.get_function(name, is_frontend).is_none(),
            "ICE: function '{name}' declared subsequentally"
        );

        let name = self.internal_function_name(name, is_frontend);
        let value = self.module().add_function(&name, r#type, linkage);

        if self.debug_info().is_some() {
            self.builder().unset_current_debug_location();
            let func_scope = match value.get_subprogram() {
                None => {
                    let fn_name = value.get_name().to_str()?;
                    let scp = self.build_function_debug_info(fn_name, 0)?;
                    value.set_subprogram(scp);
                    scp
                }
                Some(scp) => scp,
            };
            self.push_debug_scope(func_scope.as_debug_info_scope());
            let (line, column) = location.unwrap_or((0, 0));
            self.set_debug_location(line, column, Some(func_scope.as_debug_info_scope()))?;
        }

        let entry_block = self.llvm.append_basic_block(value, "entry");
        let return_block = self.llvm.append_basic_block(value, "return");

        let r#return = match return_values_length {
            0 => FunctionReturn::none(),
            1 => {
                self.set_basic_block(entry_block);
                // Use the actual return type from the function signature.
                // This allows narrowed return types (e.g., i64 instead of i256)
                // to flow through, reducing register pressure and spills.
                let alloca_type = r#type
                    .get_return_type()
                    .unwrap_or_else(|| self.word_type().as_basic_type_enum());
                let pointer = self.build_alloca(alloca_type, "return_pointer");
                FunctionReturn::primitive(pointer)
            }
            size => {
                self.set_basic_block(entry_block);
                // Use the actual return type from the function signature.
                let alloca_type = r#type.get_return_type().unwrap_or_else(|| {
                    self.structure_type(
                        vec![self.word_type().as_basic_type_enum(); size].as_slice(),
                    )
                    .as_basic_type_enum()
                });
                let pointer = self.build_alloca(alloca_type, "return_pointer");
                FunctionReturn::compound(pointer, size)
            }
        };

        let function = Function::new(
            name.clone(),
            FunctionDeclaration::new(r#type, value),
            r#return,
            entry_block,
            return_block,
        );
        Function::set_default_attributes(self.llvm, function.declaration(), &self.optimizer);
        let function = Rc::new(RefCell::new(function));
        self.functions.insert(name, function.clone());

        self.pop_debug_scope();

        Ok(function)
    }

    /// Returns a shared reference to the specified function.
    pub fn get_function(
        &self,
        name: &str,
        is_frontend: bool,
    ) -> Option<Rc<RefCell<Function<'ctx>>>> {
        self.functions
            .get(&self.internal_function_name(name, is_frontend))
            .cloned()
    }

    /// Returns a shared reference to the current active function.
    pub fn current_function(&self) -> Rc<RefCell<Function<'ctx>>> {
        self.current_function
            .clone()
            .expect("Must be declared before use")
    }

    /// Sets the current active function. If debug-info generation is enabled,
    /// constructs a debug-scope and pushes in on the scope-stack.
    pub fn set_current_function(
        &mut self,
        name: &str,
        location: Option<(u32, u32)>,
        frontend: bool,
    ) -> anyhow::Result<()> {
        let function = self.get_function(name, frontend).ok_or_else(|| {
            anyhow::anyhow!("Failed to activate an undeclared function `{}`", name)
        })?;
        self.current_function = Some(function);

        if let Some(scope) = self.current_function().borrow().get_debug_scope() {
            self.push_debug_scope(scope);
        }
        let (line, column) = location.unwrap_or_default();
        self.set_debug_location(line, column, None)?;

        Ok(())
    }

    /// Builds a debug-info scope for a function.
    pub fn build_function_debug_info(
        &self,
        name: &str,
        line_no: u32,
    ) -> anyhow::Result<inkwell::debug_info::DISubprogram<'ctx>> {
        let Some(debug_info) = self.debug_info() else {
            anyhow::bail!("expected debug-info builders");
        };
        let builder = debug_info.builder();
        let file = debug_info.compilation_unit().get_file();
        let scope = file.as_debug_info_scope();
        let flags = inkwell::debug_info::DIFlagsConstants::PUBLIC;
        let return_type = debug_info.create_word_type(Some(flags))?.as_type();
        let subroutine_type = builder.create_subroutine_type(file, Some(return_type), &[], flags);

        Ok(builder.create_function(
            scope,
            name,
            None,
            file,
            line_no,
            subroutine_type,
            false,
            true,
            1,
            flags,
            false,
        ))
    }

    /// Set the debug info location.
    ///
    /// No-op if the emitting debug info is disabled.
    ///
    /// If `scope` is `None` the top scope will be used.
    pub fn set_debug_location(
        &self,
        line: u32,
        column: u32,
        scope: Option<DIScope<'ctx>>,
    ) -> anyhow::Result<()> {
        let Some(debug_info) = self.debug_info() else {
            return Ok(());
        };
        let scope = match scope {
            Some(scp) => scp,
            None => debug_info.top_scope().expect("expected a debug-info scope"),
        };
        let location =
            debug_info
                .builder()
                .create_debug_location(self.llvm(), line, column, scope, None);

        self.builder().set_current_debug_location(location);

        Ok(())
    }

    /// Pushes a debug-info scope to the stack.
    pub fn push_debug_scope(&self, scope: DIScope<'ctx>) {
        if let Some(debug_info) = self.debug_info() {
            debug_info.push_scope(scope);
        }
    }

    /// Pops the top of the debug-info scope stack.
    pub fn pop_debug_scope(&self) {
        if let Some(debug_info) = self.debug_info() {
            debug_info.pop_scope();
        }
    }

    /// Pushes a new loop context to the stack.
    pub fn push_loop(
        &mut self,
        body_block: inkwell::basic_block::BasicBlock<'ctx>,
        continue_block: inkwell::basic_block::BasicBlock<'ctx>,
        join_block: inkwell::basic_block::BasicBlock<'ctx>,
    ) {
        self.loop_stack
            .push(Loop::new(body_block, continue_block, join_block));
    }

    /// Pops the current loop context from the stack.
    pub fn pop_loop(&mut self) {
        self.loop_stack.pop();
    }

    /// Returns the current loop context.
    pub fn r#loop(&self) -> &Loop<'ctx> {
        self.loop_stack
            .last()
            .expect("The current context is not in a loop")
    }

    /// Returns the debug info.
    pub fn debug_info(&self) -> Option<&DebugInfo<'ctx>> {
        self.debug_info.as_ref()
    }

    /// Returns the debug config reference.
    pub fn debug_config(&self) -> &DebugConfig {
        &self.debug_config
    }

    /// Appends a new basic block to the current function.
    pub fn append_basic_block(&self, name: &str) -> inkwell::basic_block::BasicBlock<'ctx> {
        self.llvm
            .append_basic_block(self.current_function().borrow().declaration().value, name)
    }

    /// Sets the current basic block.
    pub fn set_basic_block(&self, block: inkwell::basic_block::BasicBlock<'ctx>) {
        self.builder.position_at_end(block);
    }

    /// Returns the current basic block.
    pub fn basic_block(&self) -> inkwell::basic_block::BasicBlock<'ctx> {
        self.builder.get_insert_block().expect("Always exists")
    }

    /// Builds an aligned stack allocation at the function entry.
    pub fn build_alloca_at_entry<T: BasicType<'ctx> + Clone + Copy>(
        &self,
        r#type: T,
        name: &str,
    ) -> Pointer<'ctx> {
        // Allocas at the entry block coalesce into a single stack frame allocation,
        // eliminating per-alloca dynamic stack adjustment. LLVM's stack coloring pass
        // handles lifetime-based slot reuse regardless of alloca placement.
        let current_block = self.builder.get_insert_block().unwrap();
        let function = current_block.get_parent().unwrap();
        let entry_block = function.get_first_basic_block().unwrap();

        // Position at the end of the entry block (before the terminator if any)
        if let Some(terminator) = entry_block.get_terminator() {
            self.builder.position_before(&terminator);
        } else {
            self.builder.position_at_end(entry_block);
        }

        let pointer = self.builder.build_alloca(r#type, name).unwrap();
        pointer
            .as_instruction()
            .unwrap()
            .set_alignment(revive_common::BYTE_LENGTH_STACK_ALIGN as u32)
            .expect("Alignment is valid");

        // Restore insertion point
        self.builder.position_at_end(current_block);

        Pointer::new(r#type, AddressSpace::Stack, pointer)
    }

    /// Builds an aligned stack allocation at the current position.
    /// Use this if [`Self::build_alloca_at_entry`] might change program semantics.
    /// Otherwise, alloca should always be built at the function prelude!
    pub fn build_alloca<T: BasicType<'ctx> + Clone + Copy>(
        &self,
        r#type: T,
        name: &str,
    ) -> Pointer<'ctx> {
        let pointer = self.builder.build_alloca(r#type, name).unwrap();

        pointer
            .as_instruction()
            .unwrap()
            .set_alignment(revive_common::BYTE_LENGTH_STACK_ALIGN as u32)
            .expect("Alignment is valid");

        Pointer::new(r#type, AddressSpace::Stack, pointer)
    }

    /// Truncate `address` to the ethereum address length and store it as bytes on the stack.
    /// The stack allocation will be at the function entry. Returns the stack pointer.
    /// This helper should be used when passing address arguments to the runtime, ensuring correct size and endianness.
    pub fn build_address_argument_store(
        &self,
        address: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<Pointer<'ctx>> {
        let address_type = self.integer_type(revive_common::BIT_LENGTH_ETH_ADDRESS);
        let address_pointer = self
            .get_global(crate::polkavm::GLOBAL_ADDRESS_SPILL_BUFFER)?
            .into();
        let address_truncated =
            self.builder()
                .build_int_truncate(address, address_type, "address_truncated")?;
        let address_swapped = self.build_byte_swap(address_truncated.into())?;
        self.build_store(address_pointer, address_swapped)?;
        Ok(address_pointer)
    }

    /// Load the address at given pointer and zero extend it to the VM word size.
    pub fn build_load_address(
        &self,
        pointer: Pointer<'ctx>,
    ) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
        let address = self.build_byte_swap(self.build_load(pointer, "address_value")?)?;
        Ok(self
            .builder()
            .build_int_z_extend(address.into_int_value(), self.word_type(), "address_zext")?
            .into())
    }

    /// Builds a stack load instruction.
    /// Sets the alignment to 256 bits for the stack and 1 bit for the heap, parent, and child.
    pub fn build_load(
        &self,
        pointer: Pointer<'ctx>,
        name: &str,
    ) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
        match pointer.address_space {
            AddressSpace::Heap => {
                let name = <PolkaVMLoadHeapWordFunction as RuntimeFunction>::NAME;
                let declaration =
                    <PolkaVMLoadHeapWordFunction as RuntimeFunction>::declaration(self);
                let arguments = [self
                    .builder()
                    .build_ptr_to_int(pointer.value, self.xlen_type(), "offset_ptrtoint")?
                    .as_basic_value_enum()];
                Ok(self
                    .build_call(declaration, &arguments, "heap_load")
                    .unwrap_or_else(|| {
                        panic!("revive runtime function {name} should return a value")
                    }))
            }
            AddressSpace::Stack => {
                let value = self
                    .builder()
                    .build_load(pointer.r#type, pointer.value, name)?;

                self.basic_block()
                    .get_last_instruction()
                    .expect("Always exists")
                    .set_alignment(revive_common::BYTE_LENGTH_STACK_ALIGN as u32)
                    .expect("Alignment is valid");

                Ok(value)
            }
        }
    }

    /// Builds a stack store instruction.
    /// Sets the alignment to 256 bits for the stack and 1 bit for the heap, parent, and child.
    pub fn build_store<V>(&self, pointer: Pointer<'ctx>, value: V) -> anyhow::Result<()>
    where
        V: BasicValue<'ctx>,
    {
        match pointer.address_space {
            AddressSpace::Heap => {
                let declaration =
                    <PolkaVMStoreHeapWordFunction as RuntimeFunction>::declaration(self);
                let arguments = [
                    pointer.to_int(self).as_basic_value_enum(),
                    value.as_basic_value_enum(),
                ];
                self.build_call(declaration, &arguments, "heap_store");
            }
            AddressSpace::Stack => {
                let instruction = self.builder.build_store(pointer.value, value).unwrap();
                instruction
                    .set_alignment(revive_common::BYTE_LENGTH_STACK_ALIGN as u32)
                    .expect("Alignment is valid");
            }
        };

        Ok(())
    }

    /// Builds a heap load instruction without byte-swapping.
    /// Used for internal memory operations that don't escape to external code.
    pub fn build_load_native(
        &self,
        offset: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
        let name = <PolkaVMLoadHeapWordNativeFunction as RuntimeFunction>::NAME;
        let declaration = <PolkaVMLoadHeapWordNativeFunction as RuntimeFunction>::declaration(self);
        let arguments = [offset.as_basic_value_enum()];
        Ok(self
            .build_call(declaration, &arguments, "heap_load_native")
            .unwrap_or_else(|| panic!("revive runtime function {name} should return a value")))
    }

    /// Builds a heap store instruction without byte-swapping.
    /// Used for internal memory operations that don't escape to external code.
    pub fn build_store_native(
        &self,
        offset: inkwell::values::IntValue<'ctx>,
        value: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<()> {
        let declaration =
            <PolkaVMStoreHeapWordNativeFunction as RuntimeFunction>::declaration(self);
        let arguments = [offset.as_basic_value_enum(), value.as_basic_value_enum()];
        self.build_call(declaration, &arguments, "heap_store_native");
        Ok(())
    }

    /// Swap the endianness of an intvalue
    pub fn build_byte_swap(
        &self,
        value: inkwell::values::BasicValueEnum<'ctx>,
    ) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
        let intrinsic = match value.get_type().into_int_type().get_bit_width() as usize {
            revive_common::BIT_LENGTH_WORD => self.intrinsics().byte_swap_word.value,
            revive_common::BIT_LENGTH_ETH_ADDRESS => self.intrinsics().byte_swap_eth_address.value,
            _ => panic!(
                "invalid byte swap parameter: {:?} {}",
                value.get_name(),
                value.get_type()
            ),
        };
        Ok(self
            .builder()
            .build_call(intrinsic, &[value.into()], "call_byte_swap")?
            .try_as_basic_value()
            .unwrap_basic())
    }

    /// Builds a GEP instruction.
    pub fn build_gep<T>(
        &self,
        pointer: Pointer<'ctx>,
        indexes: &[inkwell::values::IntValue<'ctx>],
        element_type: T,
        name: &str,
    ) -> Pointer<'ctx>
    where
        T: BasicType<'ctx>,
    {
        let value = unsafe {
            self.builder
                .build_gep(pointer.r#type, pointer.value, indexes, name)
                .unwrap()
        };
        Pointer::new(element_type, pointer.address_space, value)
    }

    /// Builds a conditional branch.
    /// Checks if there are no other terminators in the block.
    pub fn build_conditional_branch(
        &self,
        comparison: inkwell::values::IntValue<'ctx>,
        then_block: inkwell::basic_block::BasicBlock<'ctx>,
        else_block: inkwell::basic_block::BasicBlock<'ctx>,
    ) -> anyhow::Result<()> {
        if self.basic_block().get_terminator().is_some() {
            return Ok(());
        }

        self.builder
            .build_conditional_branch(comparison, then_block, else_block)?;

        Ok(())
    }

    /// Builds an unconditional branch.
    /// Checks if there are no other terminators in the block.
    pub fn build_unconditional_branch(
        &self,
        destination_block: inkwell::basic_block::BasicBlock<'ctx>,
    ) {
        if self.basic_block().get_terminator().is_some() {
            return;
        }

        self.builder
            .build_unconditional_branch(destination_block)
            .unwrap();
    }

    /// Builds a call to a runtime API method.
    pub fn build_runtime_call(
        &self,
        name: &'static str,
        arguments: &[inkwell::values::BasicValueEnum<'ctx>],
    ) -> Option<inkwell::values::BasicValueEnum<'ctx>> {
        self.builder
            .build_direct_call(
                self.runtime_api_method(name),
                &arguments
                    .iter()
                    .copied()
                    .map(inkwell::values::BasicMetadataValueEnum::from)
                    .collect::<Vec<_>>(),
                &format!("runtime_api_{name}_return_value"),
            )
            .unwrap()
            .try_as_basic_value()
            .basic()
    }

    /// Builds a call to the runtime API `import`, where `import` is a "getter" API.
    /// This means that the supplied API method just writes back a single word.
    /// `import` is thus expect to have a single parameter, the 32 bytes output buffer,
    /// and no return value.
    pub fn build_runtime_call_to_getter(
        &self,
        import: &'static str,
    ) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
        let pointer = self.build_alloca_at_entry(self.word_type(), &format!("{import}_output"));
        self.build_runtime_call(import, &[pointer.to_int(self).into()]);
        self.build_load(pointer, import)
    }

    /// Builds a call.
    pub fn build_call(
        &self,
        function: FunctionDeclaration<'ctx>,
        arguments: &[inkwell::values::BasicValueEnum<'ctx>],
        name: &str,
    ) -> Option<inkwell::values::BasicValueEnum<'ctx>> {
        let arguments_wrapped: Vec<inkwell::values::BasicMetadataValueEnum> = arguments
            .iter()
            .copied()
            .map(inkwell::values::BasicMetadataValueEnum::from)
            .collect();
        let call_site_value = self
            .builder
            .build_indirect_call(
                function.r#type,
                function.value.as_global_value().as_pointer_value(),
                arguments_wrapped.as_slice(),
                name,
            )
            .unwrap();
        self.modify_call_site_value(arguments, call_site_value, function);
        call_site_value.try_as_basic_value().basic()
    }

    /// Sets the alignment to `1`, since all non-stack memory pages have such alignment.
    pub fn build_memcpy(
        &self,
        destination: Pointer<'ctx>,
        source: Pointer<'ctx>,
        size: inkwell::values::IntValue<'ctx>,
        name: &str,
    ) -> anyhow::Result<()> {
        let size = self.safe_truncate_int_to_xlen(size)?;

        let destination = if destination.address_space == AddressSpace::Heap {
            self.build_heap_gep(
                self.builder()
                    .build_ptr_to_int(destination.value, self.xlen_type(), name)?,
                size,
            )?
        } else {
            destination
        };

        let source = if source.address_space == AddressSpace::Heap {
            self.build_heap_gep(
                self.builder()
                    .build_ptr_to_int(source.value, self.xlen_type(), name)?,
                size,
            )?
        } else {
            source
        };

        self.builder()
            .build_memmove(destination.value, 1, source.value, 1, size)?;

        Ok(())
    }

    /// Builds a return.
    /// Checks if there are no other terminators in the block.
    pub fn build_return(&self, value: Option<&dyn BasicValue<'ctx>>) {
        if self.basic_block().get_terminator().is_some() {
            return;
        }

        self.builder.build_return(value).unwrap();
    }

    /// Builds an unreachable.
    /// Checks if there are no other terminators in the block.
    pub fn build_unreachable(&self) {
        if self.basic_block().get_terminator().is_some() {
            return;
        }

        self.builder.build_unreachable().unwrap();
    }

    /// Builds a contract exit sequence.
    pub fn build_exit(
        &self,
        flags: inkwell::values::IntValue<'ctx>,
        offset: inkwell::values::IntValue<'ctx>,
        length: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<()> {
        self.build_call(
            <Exit as RuntimeFunction>::declaration(self),
            &[flags.into(), offset.into(), length.into()],
            "exit",
        );

        Ok(())
    }

    /// Truncate a memory offset to register size, trapping if it doesn't fit.
    /// Handles xlen, word, narrow, and intermediate types.
    pub fn safe_truncate_int_to_xlen(
        &self,
        value: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<inkwell::values::IntValue<'ctx>> {
        let value_width = value.get_type().get_bit_width();
        let xlen_width = self.xlen_type().get_bit_width();
        let word_width = self.word_type().get_bit_width();

        // Already xlen-sized
        if value_width == xlen_width {
            return Ok(value);
        }

        // Narrow type (e.g., i1, i8) - zero-extend to xlen
        if value_width < xlen_width {
            return Ok(self.builder().build_int_z_extend(
                value,
                self.xlen_type(),
                "narrow_to_xlen",
            )?);
        }

        // Word type - use runtime function for safe truncation with overflow check
        if value_width == word_width {
            return Ok(self
                .build_call(
                    <WordToPointer as RuntimeFunction>::declaration(self),
                    &[value.into()],
                    "word_to_pointer",
                )
                .unwrap_or_else(|| {
                    panic!(
                        "revive runtime function {} should return a value",
                        <WordToPointer as RuntimeFunction>::NAME,
                    )
                })
                .into_int_value());
        }

        // Intermediate width (e.g., i64) - inline overflow check at original width.
        // Doing the truncate-extend-compare at i64 is much cheaper than extending
        // to i256 first: on 32-bit PVM, i64 comparison is 2 register pairs vs
        // i256 comparison requiring 8 words. This saves ~10 instructions per check.
        let truncated =
            self.builder()
                .build_int_truncate(value, self.xlen_type(), "offset_truncated")?;
        let extended =
            self.builder()
                .build_int_z_extend(truncated, value.get_type(), "offset_extended")?;
        let is_overflow = self.builder().build_int_compare(
            inkwell::IntPredicate::NE,
            value,
            extended,
            "compare_truncated_extended",
        )?;

        let block_continue = self.append_basic_block("offset_pointer_ok");
        let block_invalid = self.append_basic_block("offset_pointer_overflow");
        self.build_conditional_branch(is_overflow, block_invalid, block_continue)?;

        self.set_basic_block(block_invalid);
        self.build_runtime_call(revive_runtime_api::polkavm_imports::INVALID, &[]);
        self.build_unreachable();

        self.set_basic_block(block_continue);
        Ok(truncated)
    }

    /// Clip a memory offset to the maximum value that fits into a register.
    /// Handles xlen, word, narrow, and intermediate types.
    pub fn clip_to_xlen(
        &self,
        value: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<inkwell::values::IntValue<'ctx>> {
        let value_width = value.get_type().get_bit_width();
        let xlen_width = self.xlen_type().get_bit_width();
        let word_width = self.word_type().get_bit_width();

        // Already xlen-sized - no clipping needed
        if value_width == xlen_width {
            return Ok(value);
        }

        // Narrow type - zero-extend to xlen (no overflow possible)
        if value_width < xlen_width {
            return Ok(self.builder().build_int_z_extend(
                value,
                self.xlen_type(),
                "narrow_to_xlen",
            )?);
        }

        // Wider type - check for overflow and clip
        // For word-type values, use word_type to maintain original codegen
        // For intermediate types (e.g., i64), use the value's type
        let clipped = self.xlen_type().const_all_ones();
        let comparison_type = if value_width == word_width {
            self.word_type()
        } else {
            value.get_type()
        };
        let is_overflow = self.builder().build_int_compare(
            inkwell::IntPredicate::UGT,
            value,
            self.builder()
                .build_int_z_extend(clipped, comparison_type, "value_clipped")?,
            "is_value_overflow",
        )?;
        let truncated =
            self.builder()
                .build_int_truncate(value, self.xlen_type(), "value_truncated")?;
        Ok(self
            .builder()
            .build_select(is_overflow, clipped, truncated, "value")?
            .into_int_value())
    }

    /// Build a call to PolkaVM `sbrk` for extending the heap from offset by `size`.
    /// The allocation is aligned to 32 bytes.
    ///
    /// This emulates the EVM linear memory until the runtime supports metered memory.
    pub fn build_sbrk(
        &self,
        offset: inkwell::values::IntValue<'ctx>,
        size: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<inkwell::values::PointerValue<'ctx>> {
        let call_site_value = self.builder().build_call(
            <PolkaVMSbrkFunction as RuntimeFunction>::declaration(self).function_value(),
            &[offset.into(), size.into()],
            "alloc_start",
        )?;

        call_site_value.add_attribute(
            inkwell::attributes::AttributeLoc::Return,
            self.llvm
                .create_enum_attribute(Attribute::NonNull as u32, 0),
        );
        call_site_value.add_attribute(
            inkwell::attributes::AttributeLoc::Return,
            self.llvm
                .create_enum_attribute(Attribute::NoUndef as u32, 0),
        );

        Ok(call_site_value
            .try_as_basic_value()
            .unwrap_basic()
            .into_pointer_value())
    }

    /// Build a call to PolkaVM `msize` for querying the linear memory size.
    pub fn build_msize(&self) -> anyhow::Result<inkwell::values::IntValue<'ctx>> {
        Ok(self
            .get_global_value(crate::polkavm::GLOBAL_HEAP_SIZE)?
            .into_int_value())
    }

    /// Returns a pointer to `offset` into the heap, allocating
    /// enough memory if `offset + length` would be out of bounds.
    /// # Panics
    /// Assumes `offset` and `length` to be a register sized value.
    pub fn build_heap_gep(
        &self,
        offset: inkwell::values::IntValue<'ctx>,
        length: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<Pointer<'ctx>> {
        assert_eq!(offset.get_type(), self.xlen_type());
        assert_eq!(length.get_type(), self.xlen_type());

        let pointer = self.build_sbrk(offset, length)?;
        Ok(Pointer::new(self.byte_type(), AddressSpace::Stack, pointer))
    }

    /// Returns a pointer to `offset` into the heap WITHOUT calling sbrk.
    /// This is safe only for offsets known to be within the statically
    /// pre-allocated region (e.g., the scratch area at 0x00-0x7f including
    /// the free memory pointer slot at 0x40).
    ///
    /// # Panics
    /// Assumes `offset` to be a register sized value.
    pub fn build_heap_gep_unchecked(
        &self,
        offset: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<Pointer<'ctx>> {
        assert_eq!(offset.get_type(), self.xlen_type());
        let heap_global: Pointer<'ctx> =
            self.get_global(crate::polkavm::GLOBAL_HEAP_MEMORY)?.into();
        let pointer = self.build_gep(
            heap_global,
            &[self.xlen_type().const_zero(), offset],
            self.byte_type(),
            "heap_unchecked_ptr",
        );
        Ok(Pointer::new(
            self.byte_type(),
            AddressSpace::Stack,
            pointer.value,
        ))
    }

    /// Ensures the heap size (msize) is at least `min_size`.
    /// This emits a branchless max(current_msize, min_size) update.
    /// Used after native stores that bypass sbrk to keep msize consistent.
    pub fn ensure_heap_size(
        &self,
        min_size: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<()> {
        let current = self
            .get_global_value(crate::polkavm::GLOBAL_HEAP_SIZE)?
            .into_int_value();
        let needs_update = self.builder().build_int_compare(
            inkwell::IntPredicate::UGT,
            min_size,
            current,
            "msize_needs_update",
        )?;
        let new_size = self
            .builder()
            .build_select(needs_update, min_size, current, "msize_new")?
            .into_int_value();
        let heap_size_global = self.get_global(crate::polkavm::GLOBAL_HEAP_SIZE)?;
        self.build_store(heap_size_global.into(), new_size)?;
        Ok(())
    }

    /// Returns a boolean type constant.
    pub fn bool_const(&self, value: bool) -> inkwell::values::IntValue<'ctx> {
        self.bool_type().const_int(u64::from(value), false)
    }

    /// Returns an integer type constant.
    pub fn integer_const(&self, bit_length: usize, value: u64) -> inkwell::values::IntValue<'ctx> {
        self.integer_type(bit_length).const_int(value, false)
    }

    /// Returns a word type constant.
    pub fn word_const(&self, value: u64) -> inkwell::values::IntValue<'ctx> {
        self.word_type().const_int(value, false)
    }

    /// Returns a word type undefined value.
    pub fn word_undef(&self) -> inkwell::values::IntValue<'ctx> {
        self.word_type().get_undef()
    }

    /// Returns a word type constant from a decimal string.
    pub fn word_const_str_dec(&self, value: &str) -> inkwell::values::IntValue<'ctx> {
        self.word_type()
            .const_int_from_string(value, inkwell::types::StringRadix::Decimal)
            .unwrap_or_else(|| panic!("Invalid string constant `{value}`"))
    }

    /// Returns a word type constant from a hexadecimal string.
    pub fn word_const_str_hex(&self, value: &str) -> inkwell::values::IntValue<'ctx> {
        self.word_type()
            .const_int_from_string(
                value.strip_prefix("0x").unwrap_or(value),
                inkwell::types::StringRadix::Hexadecimal,
            )
            .unwrap_or_else(|| panic!("Invalid string constant `{value}`"))
    }

    /// Returns the void type.
    pub fn void_type(&self) -> inkwell::types::VoidType<'ctx> {
        self.llvm.void_type()
    }

    /// Returns the boolean type.
    pub fn bool_type(&self) -> inkwell::types::IntType<'ctx> {
        self.llvm.bool_type()
    }

    /// Returns the default byte type.
    pub fn byte_type(&self) -> inkwell::types::IntType<'ctx> {
        self.llvm
            .custom_width_int_type(revive_common::BIT_LENGTH_BYTE as u32)
    }

    /// Returns the integer type of the specified bit-length.
    pub fn integer_type(&self, bit_length: usize) -> inkwell::types::IntType<'ctx> {
        self.llvm.custom_width_int_type(bit_length as u32)
    }

    /// Returns the XLEN witdh sized type.
    pub fn xlen_type(&self) -> inkwell::types::IntType<'ctx> {
        self.llvm.custom_width_int_type(crate::polkavm::XLEN as u32)
    }

    /// Returns the PolkaVM native register width sized type.
    pub fn register_type(&self) -> inkwell::types::IntType<'ctx> {
        self.llvm
            .custom_width_int_type(revive_common::BIT_LENGTH_X64 as u32)
    }

    /// Returns the sentinel pointer value.
    pub fn sentinel_pointer(&self) -> Pointer<'ctx> {
        let sentinel_pointer = self
            .xlen_type()
            .const_all_ones()
            .const_to_pointer(self.llvm().ptr_type(Default::default()));

        Pointer::new(
            sentinel_pointer.get_type(),
            AddressSpace::Stack,
            sentinel_pointer,
        )
    }

    /// Returns the runtime value width sized type.
    pub fn value_type(&self) -> inkwell::types::IntType<'ctx> {
        self.llvm
            .custom_width_int_type(revive_common::BIT_LENGTH_VALUE as u32)
    }

    /// Returns the default word type.
    pub fn word_type(&self) -> inkwell::types::IntType<'ctx> {
        self.llvm
            .custom_width_int_type(revive_common::BIT_LENGTH_WORD as u32)
    }

    /// Returns the array type with the specified length.
    pub fn array_type<T>(&self, element_type: T, length: usize) -> inkwell::types::ArrayType<'ctx>
    where
        T: BasicType<'ctx>,
    {
        element_type.array_type(length as u32)
    }

    /// Returns the structure type with specified fields.
    pub fn structure_type<T>(&self, field_types: &[T]) -> inkwell::types::StructType<'ctx>
    where
        T: BasicType<'ctx>,
    {
        let field_types: Vec<inkwell::types::BasicTypeEnum<'ctx>> =
            field_types.iter().map(T::as_basic_type_enum).collect();
        self.llvm.struct_type(field_types.as_slice(), false)
    }

    /// Returns a Yul function type with the specified arguments and number of return values.
    /// All return values use word_type (i256). For narrowed return types, use
    /// `function_type_with_returns`.
    pub fn function_type<T>(
        &self,
        argument_types: Vec<T>,
        return_values_size: usize,
    ) -> inkwell::types::FunctionType<'ctx>
    where
        T: BasicType<'ctx>,
    {
        let argument_types: Vec<inkwell::types::BasicMetadataTypeEnum> = argument_types
            .as_slice()
            .iter()
            .map(T::as_basic_type_enum)
            .map(inkwell::types::BasicMetadataTypeEnum::from)
            .collect();
        match return_values_size {
            0 => self
                .llvm
                .void_type()
                .fn_type(argument_types.as_slice(), false),
            1 => self.word_type().fn_type(argument_types.as_slice(), false),
            size => self
                .structure_type(vec![self.word_type().as_basic_type_enum(); size].as_slice())
                .fn_type(argument_types.as_slice(), false),
        }
    }

    /// Returns a function type with explicit return types instead of word_type.
    /// Used by the newyork codegen for functions with narrowed return types.
    pub fn function_type_with_returns<T>(
        &self,
        argument_types: Vec<T>,
        return_types: &[inkwell::types::IntType<'ctx>],
    ) -> inkwell::types::FunctionType<'ctx>
    where
        T: BasicType<'ctx>,
    {
        let argument_types: Vec<inkwell::types::BasicMetadataTypeEnum> = argument_types
            .as_slice()
            .iter()
            .map(T::as_basic_type_enum)
            .map(inkwell::types::BasicMetadataTypeEnum::from)
            .collect();
        match return_types.len() {
            0 => self
                .llvm
                .void_type()
                .fn_type(argument_types.as_slice(), false),
            1 => return_types[0].fn_type(argument_types.as_slice(), false),
            _ => {
                let field_types: Vec<inkwell::types::BasicTypeEnum> = return_types
                    .iter()
                    .map(|t| t.as_basic_type_enum())
                    .collect();
                self.structure_type(&field_types)
                    .fn_type(argument_types.as_slice(), false)
            }
        }
    }

    /// Modifies the call site value, setting the default attributes.
    /// The attributes only affect the LLVM optimizations.
    pub fn modify_call_site_value(
        &self,
        arguments: &[inkwell::values::BasicValueEnum<'ctx>],
        call_site_value: inkwell::values::CallSiteValue<'ctx>,
        function: FunctionDeclaration<'ctx>,
    ) {
        for (index, argument) in arguments.iter().enumerate() {
            if argument.is_pointer_value() {
                call_site_value.set_alignment_attribute(
                    inkwell::attributes::AttributeLoc::Param(index as u32),
                    revive_common::BYTE_LENGTH_STACK_ALIGN as u32,
                );
                call_site_value.add_attribute(
                    inkwell::attributes::AttributeLoc::Param(index as u32),
                    self.llvm
                        .create_enum_attribute(Attribute::NoAlias as u32, 0),
                );
                call_site_value.add_attribute(
                    inkwell::attributes::AttributeLoc::Param(index as u32),
                    self.llvm
                        .create_enum_attribute(Attribute::Captures as u32, 0), // captures(none)
                );
                call_site_value.add_attribute(
                    inkwell::attributes::AttributeLoc::Param(index as u32),
                    self.llvm.create_enum_attribute(Attribute::NoFree as u32, 0),
                );
                if Some(argument.get_type()) == function.r#type.get_return_type() {
                    if function
                        .r#type
                        .get_return_type()
                        .map(|r#type| {
                            r#type.into_pointer_type().get_address_space()
                                == AddressSpace::Stack.into()
                        })
                        .unwrap_or_default()
                    {
                        call_site_value.add_attribute(
                            inkwell::attributes::AttributeLoc::Param(index as u32),
                            self.llvm
                                .create_enum_attribute(Attribute::Returned as u32, 0),
                        );
                    }
                    call_site_value.add_attribute(
                        inkwell::attributes::AttributeLoc::Param(index as u32),
                        self.llvm.create_enum_attribute(
                            Attribute::Dereferenceable as u32,
                            (revive_common::BIT_LENGTH_WORD * 2) as u64,
                        ),
                    );
                    call_site_value.add_attribute(
                        inkwell::attributes::AttributeLoc::Return,
                        self.llvm.create_enum_attribute(
                            Attribute::Dereferenceable as u32,
                            (revive_common::BIT_LENGTH_WORD * 2) as u64,
                        ),
                    );
                }
                call_site_value.add_attribute(
                    inkwell::attributes::AttributeLoc::Param(index as u32),
                    self.llvm
                        .create_enum_attribute(Attribute::NonNull as u32, 0),
                );
                call_site_value.add_attribute(
                    inkwell::attributes::AttributeLoc::Param(index as u32),
                    self.llvm
                        .create_enum_attribute(Attribute::NoUndef as u32, 0),
                );
            }
        }

        if function
            .r#type
            .get_return_type()
            .map(|r#type| r#type.is_pointer_type())
            .unwrap_or_default()
        {
            call_site_value.set_alignment_attribute(
                inkwell::attributes::AttributeLoc::Return,
                revive_common::BYTE_LENGTH_STACK_ALIGN as u32,
            );
            call_site_value.add_attribute(
                inkwell::attributes::AttributeLoc::Return,
                self.llvm
                    .create_enum_attribute(Attribute::NoAlias as u32, 0),
            );
            call_site_value.add_attribute(
                inkwell::attributes::AttributeLoc::Return,
                self.llvm
                    .create_enum_attribute(Attribute::NonNull as u32, 0),
            );
            call_site_value.add_attribute(
                inkwell::attributes::AttributeLoc::Return,
                self.llvm
                    .create_enum_attribute(Attribute::NoUndef as u32, 0),
            );
        }
    }

    /// Sets the Solidity data.
    pub fn set_solidity_data(&mut self, data: SolidityData) {
        self.solidity_data = Some(data);
    }

    /// Returns the Solidity data reference.
    /// # Panics
    /// If the Solidity data has not been initialized.
    pub fn solidity(&self) -> &SolidityData {
        self.solidity_data
            .as_ref()
            .expect("The Solidity data must have been initialized")
    }

    /// Returns the Solidity data mutable reference.
    /// # Panics
    /// If the Solidity data has not been initialized.
    pub fn solidity_mut(&mut self) -> &mut SolidityData {
        self.solidity_data
            .as_mut()
            .expect("The Solidity data must have been initialized")
    }

    /// Sets the Yul data.
    pub fn set_yul_data(&mut self, data: YulData) {
        self.yul_data = Some(data);
    }

    /// Returns the Yul data reference.
    /// # Panics
    /// If the Yul data has not been initialized.
    pub fn yul(&self) -> Option<&YulData> {
        self.yul_data.as_ref()
    }

    /// Returns the current number of immutables values in the contract.
    /// If the size is set manually, then it is returned. Otherwise, the number of elements in
    /// the identifier-to-offset mapping tree is returned.
    pub fn immutables_size(&self) -> anyhow::Result<usize> {
        if let Some(solidity) = self.solidity_data.as_ref() {
            Ok(solidity.immutables_size())
        } else {
            anyhow::bail!("The immutable size data is not available");
        }
    }

    pub fn optimizer_settings(&self) -> &OptimizerSettings {
        self.optimizer.settings()
    }

    pub fn heap_size(&self) -> inkwell::values::IntValue<'ctx> {
        self.xlen_type().const_int(
            self.memory_config
                .heap_size
                .unwrap_or(PolkaVMDefaultHeapMemorySize) as u64,
            false,
        )
    }

    /// Returns the internal function name.
    fn internal_function_name(&self, name: &str, is_frontend: bool) -> String {
        if is_frontend {
            format!("{name}_{}", self.code_type().unwrap())
        } else {
            name.to_string()
        }
    }

    /// Narrows large integer div/rem instructions whose operands provably fit
    /// in a smaller type. This preserves the `MinSize` attribute on functions
    /// that would otherwise have it stripped by `strip_minsize_for_divrem`.
    ///
    /// LLVM's `DivRemNarrowing` pass sometimes fails to narrow div/rem in
    /// large functions after inlining. This runs as a safety net after
    /// optimization to catch those cases.
    fn narrow_divrem_instructions(&self) {
        let builder = self.llvm.create_builder();

        for func in self.module().get_functions() {
            let mut to_narrow = Vec::new();

            for bb in func.get_basic_blocks() {
                for inst in bb.get_instructions() {
                    let is_divrem = matches!(
                        inst.get_opcode(),
                        InstructionOpcode::UDiv
                            | InstructionOpcode::SDiv
                            | InstructionOpcode::URem
                            | InstructionOpcode::SRem
                    );
                    if !is_divrem {
                        continue;
                    }
                    if inst.get_type().into_int_type().get_bit_width() < 256 {
                        continue;
                    }

                    let lhs = inst.get_operand(0).and_then(|op| op.value());
                    let rhs = inst.get_operand(1).and_then(|op| op.value());

                    if let (Some(lhs), Some(rhs)) = (lhs, rhs) {
                        let lhs_width = Self::provable_bit_width(lhs);
                        let rhs_width = Self::provable_bit_width(rhs);

                        if let (Some(lw), Some(rw)) = (lhs_width, rhs_width) {
                            let narrow_width = Self::round_up_bit_width(lw.max(rw));
                            if narrow_width < 256 {
                                to_narrow.push((inst, narrow_width));
                            }
                        }
                    }
                }
            }

            for (inst, narrow_width) in to_narrow {
                let lhs = inst
                    .get_operand(0)
                    .unwrap()
                    .value()
                    .unwrap()
                    .into_int_value();
                let rhs = inst
                    .get_operand(1)
                    .unwrap()
                    .value()
                    .unwrap()
                    .into_int_value();
                let wide_type = inst.get_type().into_int_type();
                let narrow_type = self.llvm.custom_width_int_type(narrow_width);

                builder.position_before(&inst);

                let lhs_trunc = builder.build_int_truncate(lhs, narrow_type, "").unwrap();
                let rhs_trunc = builder.build_int_truncate(rhs, narrow_type, "").unwrap();

                let narrow_result = match inst.get_opcode() {
                    InstructionOpcode::UDiv => builder
                        .build_int_unsigned_div(lhs_trunc, rhs_trunc, "")
                        .unwrap(),
                    InstructionOpcode::SDiv => builder
                        .build_int_signed_div(lhs_trunc, rhs_trunc, "")
                        .unwrap(),
                    InstructionOpcode::URem => builder
                        .build_int_unsigned_rem(lhs_trunc, rhs_trunc, "")
                        .unwrap(),
                    InstructionOpcode::SRem => builder
                        .build_int_signed_rem(lhs_trunc, rhs_trunc, "")
                        .unwrap(),
                    _ => unreachable!(),
                };

                let wide_result = if matches!(
                    inst.get_opcode(),
                    InstructionOpcode::SDiv | InstructionOpcode::SRem
                ) {
                    builder
                        .build_int_s_extend(narrow_result, wide_type, "")
                        .unwrap()
                } else {
                    builder
                        .build_int_z_extend(narrow_result, wide_type, "")
                        .unwrap()
                };

                let wide_inst = wide_result.as_instruction().unwrap();
                inst.replace_all_uses_with(&wide_inst);
                inst.erase_from_basic_block();
            }
        }
    }

    /// Returns the provable bit width of a value, if it can be determined.
    ///
    /// Checks for:
    /// - Constants: bit width needed to represent the value
    /// - `and %x, mask`: bit width of the mask
    /// - `zext from smaller_type`: bit width of the source type
    fn provable_bit_width(value: inkwell::values::BasicValueEnum) -> Option<u32> {
        let int_val = value.into_int_value();

        // Check if it's a constant
        if int_val.is_const() {
            return Self::constant_bit_width(int_val);
        }

        // Check if it's an instruction we can analyze
        let inst = int_val.as_instruction()?;
        match inst.get_opcode() {
            InstructionOpcode::And => {
                // and %x, mask - result fits in the width of the mask
                let op0 = inst.get_operand(0)?.value()?.into_int_value();
                let op1 = inst.get_operand(1)?.value()?.into_int_value();

                // Check if either operand is a constant mask
                if op1.is_const() {
                    Self::constant_bit_width(op1)
                } else if op0.is_const() {
                    Self::constant_bit_width(op0)
                } else {
                    None
                }
            }
            InstructionOpcode::ZExt => {
                // zext from smaller type - fits in the source width
                let source = inst.get_operand(0)?.value()?.into_int_value();
                Some(source.get_type().get_bit_width())
            }
            InstructionOpcode::Trunc => {
                // trunc to smaller type - fits in the result width
                Some(inst.get_type().into_int_type().get_bit_width())
            }
            _ => None,
        }
    }

    /// Returns the minimum number of bits needed to represent a constant integer.
    /// Handles wide types (> 64 bits) by truncating to i64 and verifying roundtrip.
    fn constant_bit_width(int_val: inkwell::values::IntValue) -> Option<u32> {
        // For types <= 64 bits, use the direct API
        if let Some(val) = int_val.get_zero_extended_constant() {
            return Some(if val == 0 {
                1
            } else {
                64 - val.leading_zeros()
            });
        }

        // For wider types (e.g., i256), truncate to i64 and verify roundtrip.
        // LLVM constants are interned so pointer equality works for comparison.
        let wide_type = int_val.get_type();
        if wide_type.get_bit_width() > 64 {
            let i64_type = wide_type.get_context().i64_type();
            let truncated = int_val.const_truncate(i64_type);
            if let Some(val) = truncated.get_zero_extended_constant() {
                // Reconstruct at the original width - if it matches, value fits in u64
                let reconstructed = wide_type.const_int(val, false);
                if reconstructed == int_val {
                    return Some(if val == 0 {
                        1
                    } else {
                        64 - val.leading_zeros()
                    });
                }
            }
        }

        None
    }

    /// Rounds a bit width up to the next standard integer type width.
    fn round_up_bit_width(bits: u32) -> u32 {
        if bits <= 8 {
            8
        } else if bits <= 16 {
            16
        } else if bits <= 32 {
            32
        } else if bits <= 64 {
            64
        } else if bits <= 128 {
            128
        } else {
            256
        }
    }

    /// Scans all functions in the module and removes the `MinSize` attribute
    /// if the function contains any large sdiv, udiv, srem, urem instructions with either unknown
    /// NOTE: The check here could be relaxed by checking denominator: if the denominator is
    /// unknown or is a power-of-2 constant, then need to strip the `minsize` attribute; otherwise
    /// instruction can be ignored as backend will expand it correctly.
    fn strip_minsize_for_divrem(&self) {
        self.module().get_functions().for_each(|func| {
            let has_divrem = func.get_basic_block_iter().any(|b| {
                b.get_instructions().any(|inst| match inst.get_opcode() {
                    InstructionOpcode::SDiv
                    | InstructionOpcode::UDiv
                    | InstructionOpcode::SRem
                    | InstructionOpcode::URem => {
                        inst.get_type().into_int_type().get_bit_width() >= 256
                    }
                    _ => false,
                })
            });
            if has_divrem
                && func
                    .get_enum_attribute(
                        inkwell::attributes::AttributeLoc::Function,
                        Attribute::MinSize as u32,
                    )
                    .is_some()
            {
                func.remove_enum_attribute(
                    inkwell::attributes::AttributeLoc::Function,
                    Attribute::MinSize as u32,
                );
            }
        });
    }
}
