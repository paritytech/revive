//! The LLVM IR generator context.

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

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use inkwell::debug_info::AsDIScope;
use inkwell::debug_info::DIScope;
use inkwell::types::BasicType;
use inkwell::values::BasicValue;
use revive_solc_json_interface::SolcStandardJsonInputSettingsPolkaVMMemory;

use crate::optimizer::settings::Settings as OptimizerSettings;
use crate::optimizer::Optimizer;
use crate::polkavm::DebugConfig;
use crate::polkavm::Dependency;
use crate::target_machine::target::Target;
use crate::target_machine::TargetMachine;
use crate::PolkaVMLoadHeapWordFunction;
use crate::PolkaVMSbrkFunction;
use crate::PolkaVMStoreHeapWordFunction;

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

/// The LLVM IR generator context.
/// It is a not-so-big god-like object glueing all the compilers' complexity and act as an adapter
/// and a superstructure over the inner `inkwell` LLVM context.
pub struct Context<'ctx, D>
where
    D: Dependency + Clone,
{
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
    /// The extra LLVM arguments that were used during target initialization.
    llvm_arguments: &'ctx [String],
    /// The PVM memory configuration.
    memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,

    /// The project dependency manager. It can be any entity implementing the trait.
    /// The manager is used to get information about contracts and their dependencies during
    /// the multi-threaded compilation process.
    dependency_manager: Option<D>,
    /// Whether to append the metadata hash at the end of bytecode.
    include_metadata_hash: bool,
    /// The debug info of the current module.
    debug_info: Option<DebugInfo<'ctx>>,
    /// The debug configuration telling whether to dump the needed IRs.
    debug_config: DebugConfig,

    /// The Solidity data.
    solidity_data: Option<SolidityData>,
    /// The Yul data.
    yul_data: Option<YulData>,
}

impl<'ctx, D> Context<'ctx, D>
where
    D: Dependency + Clone,
{
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
                .set_linkage(inkwell::module::Linkage::External);
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        llvm: &'ctx inkwell::context::Context,
        module: inkwell::module::Module<'ctx>,
        optimizer: Optimizer,
        dependency_manager: Option<D>,
        include_metadata_hash: bool,
        debug_config: DebugConfig,
        llvm_arguments: &'ctx [String],
        memory_config: SolcStandardJsonInputSettingsPolkaVMMemory,
    ) -> Self {
        Self::set_data_layout(llvm, &module);
        Self::link_stdlib_module(llvm, &module);
        Self::link_polkavm_imports(llvm, &module);
        Self::set_polkavm_stack_size(llvm, &module, memory_config.stack_size);
        Self::set_module_flags(llvm, &module);

        let intrinsics = Intrinsics::new(llvm, &module);
        let llvm_runtime = LLVMRuntime::new(llvm, &module, &optimizer);
        let debug_info = debug_config.emit_debug_info.then(|| {
            let debug_info = DebugInfo::new(&module);
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
            llvm_arguments,
            memory_config,

            dependency_manager,
            include_metadata_hash,

            debug_info,
            debug_config,

            solidity_data: None,
            yul_data: None,
        }
    }

    /// Builds the LLVM IR module, returning the build artifacts.
    pub fn build(
        mut self,
        contract_path: &str,
        metadata_hash: Option<[u8; revive_common::BYTE_LENGTH_WORD]>,
    ) -> anyhow::Result<Build> {
        let module_clone = self.module.clone();

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

        let shared_object = revive_linker::link(buffer.as_slice())?;

        self.debug_config
            .dump_object(contract_path, &shared_object)?;

        let polkavm_bytecode =
            revive_linker::polkavm_linker(shared_object, !self.debug_config().emit_debug_info)?;

        let build = match crate::polkavm::build_assembly_text(
            contract_path,
            &polkavm_bytecode,
            metadata_hash,
            self.debug_config(),
        ) {
            Ok(build) => build,
            Err(_error)
                if self.optimizer.settings() != &OptimizerSettings::size()
                    && self.optimizer.settings().is_fallback_to_size_enabled() =>
            {
                self.optimizer = Optimizer::new(OptimizerSettings::size());
                self.module = module_clone;
                self.build(contract_path, metadata_hash)?
            }
            Err(error) => Err(error)?,
        };

        Ok(build)
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

    /// Declare an external global.
    pub fn declare_global<T>(&mut self, name: &str, r#type: T, address_space: AddressSpace)
    where
        T: BasicType<'ctx> + Clone + Copy,
    {
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
    ) -> anyhow::Result<Rc<RefCell<Function<'ctx>>>> {
        let value = self.module().add_function(name, r#type, linkage);

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
            self.set_debug_location(0, 0, Some(func_scope.as_debug_info_scope()))?;
        }

        let entry_block = self.llvm.append_basic_block(value, "entry");
        let return_block = self.llvm.append_basic_block(value, "return");

        let r#return = match return_values_length {
            0 => FunctionReturn::none(),
            1 => {
                self.set_basic_block(entry_block);
                let pointer = self.build_alloca(self.word_type(), "return_pointer");
                FunctionReturn::primitive(pointer)
            }
            size => {
                self.set_basic_block(entry_block);
                let pointer = self.build_alloca(
                    self.structure_type(
                        vec![self.word_type().as_basic_type_enum(); size].as_slice(),
                    ),
                    "return_pointer",
                );
                FunctionReturn::compound(pointer, size)
            }
        };

        let function = Function::new(
            name.to_owned(),
            FunctionDeclaration::new(r#type, value),
            r#return,
            entry_block,
            return_block,
        );
        Function::set_default_attributes(self.llvm, function.declaration(), &self.optimizer);
        let function = Rc::new(RefCell::new(function));
        self.functions.insert(name.to_string(), function.clone());

        self.pop_debug_scope();

        Ok(function)
    }

    /// Returns a shared reference to the specified function.
    pub fn get_function(&self, name: &str) -> Option<Rc<RefCell<Function<'ctx>>>> {
        self.functions.get(name).cloned()
    }

    /// Returns a shared reference to the current active function.
    pub fn current_function(&self) -> Rc<RefCell<Function<'ctx>>> {
        self.current_function
            .clone()
            .expect("Must be declared before use")
    }

    /// Sets the current active function. If debug-info generation is enabled,
    /// constructs a debug-scope and pushes in on the scope-stack.
    pub fn set_current_function(&mut self, name: &str, line: Option<u32>) -> anyhow::Result<()> {
        let function = self.functions.get(name).cloned().ok_or_else(|| {
            anyhow::anyhow!("Failed to activate an undeclared function `{}`", name)
        })?;
        self.current_function = Some(function);

        if let Some(scope) = self.current_function().borrow().get_debug_scope() {
            self.push_debug_scope(scope);
        }
        self.set_debug_location(line.unwrap_or_default(), 0, None)?;

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

    /// Compiles a contract dependency, if the dependency manager is set.
    pub fn compile_dependency(&mut self, name: &str) -> anyhow::Result<String> {
        self.dependency_manager
            .to_owned()
            .ok_or_else(|| anyhow::anyhow!("The dependency manager is unset"))
            .and_then(|manager| {
                Dependency::compile(
                    manager,
                    name,
                    self.optimizer.settings().to_owned(),
                    self.include_metadata_hash,
                    self.debug_config.clone(),
                    self.llvm_arguments,
                    self.memory_config,
                )
            })
    }

    /// Gets a full contract_path from the dependency manager.
    pub fn resolve_path(&self, identifier: &str) -> anyhow::Result<String> {
        self.dependency_manager
            .to_owned()
            .ok_or_else(|| anyhow::anyhow!("The dependency manager is unset"))
            .and_then(|manager| {
                let full_path = manager.resolve_path(identifier)?;
                Ok(full_path)
            })
    }

    /// Gets a deployed library address from the dependency manager.
    pub fn resolve_library(&self, path: &str) -> anyhow::Result<inkwell::values::IntValue<'ctx>> {
        self.dependency_manager
            .to_owned()
            .ok_or_else(|| anyhow::anyhow!("The dependency manager is unset"))
            .and_then(|manager| {
                let address = manager.resolve_library(path)?;
                let address = self.word_const_str_hex(address.as_str());
                Ok(address)
            })
    }

    /// Extracts the dependency manager.
    pub fn take_dependency_manager(&mut self) -> D {
        self.dependency_manager
            .take()
            .expect("The dependency manager is unset")
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
        let current_block = self.basic_block();
        let entry_block = self.current_function().borrow().entry_block();

        match entry_block.get_first_instruction() {
            Some(instruction) => self.builder().position_before(&instruction),
            None => self.builder().position_at_end(entry_block),
        }

        let pointer = self.build_alloca(r#type, name);
        self.set_basic_block(current_block);
        pointer
    }

    /// Builds an aligned stack allocation at the current position.
    /// Use this if [`build_alloca_at_entry`] might change program semantics.
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
        let address = self.build_byte_swap(self.build_load(pointer, "address_pointer")?)?;
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
                let name = <PolkaVMLoadHeapWordFunction as RuntimeFunction<D>>::NAME;
                let declaration =
                    <PolkaVMLoadHeapWordFunction as RuntimeFunction<D>>::declaration(self);
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
                    <PolkaVMStoreHeapWordFunction as RuntimeFunction<D>>::declaration(self);
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
            .left()
            .unwrap())
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
            .left()
    }

    /// Builds a call to the runtime API `import`, where `import` is a "getter" API.
    /// This means that the supplied API method just writes back a single word.
    /// `import` is thus expect to have a single parameter, the 32 bytes output buffer,
    /// and no return value.
    pub fn build_runtime_call_to_getter(
        &self,
        import: &'static str,
    ) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>>
    where
        D: Dependency + Clone,
    {
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
        call_site_value.try_as_basic_value().left()
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
            <Exit as RuntimeFunction<D>>::declaration(self),
            &[flags.into(), offset.into(), length.into()],
            "exit",
        );

        Ok(())
    }

    /// Truncate a memory offset to register size, trapping if it doesn't fit.
    pub fn safe_truncate_int_to_xlen(
        &self,
        value: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<inkwell::values::IntValue<'ctx>> {
        if value.get_type() == self.xlen_type() {
            return Ok(value);
        }
        assert_eq!(
            value.get_type(),
            self.word_type(),
            "expected XLEN or WORD sized int type for memory offset",
        );

        Ok(self
            .build_call(
                <WordToPointer as RuntimeFunction<D>>::declaration(self),
                &[value.into()],
                "word_to_pointer",
            )
            .unwrap_or_else(|| {
                panic!(
                    "revive runtime function {} should return a value",
                    <WordToPointer as RuntimeFunction<D>>::NAME,
                )
            })
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
            <PolkaVMSbrkFunction as RuntimeFunction<D>>::declaration(self).function_value(),
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
            .left()
            .unwrap_or_else(|| {
                panic!(
                    "revive runtime function {} should return a value",
                    <PolkaVMSbrkFunction as RuntimeFunction<D>>::NAME,
                )
            })
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
                        .create_enum_attribute(Attribute::NoCapture as u32, 0),
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
    pub fn yul(&self) -> &YulData {
        self.yul_data
            .as_ref()
            .expect("The Yul data must have been initialized")
    }

    /// Returns the Yul data mutable reference.
    /// # Panics
    /// If the Yul data has not been initialized.
    pub fn yul_mut(&mut self) -> &mut YulData {
        self.yul_data
            .as_mut()
            .expect("The Yul data must have been initialized")
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
        self.xlen_type()
            .const_int(self.memory_config.heap_size as u64, false)
    }
}
