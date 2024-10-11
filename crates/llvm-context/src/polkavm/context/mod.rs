//! The LLVM IR generator context.

pub mod address_space;
pub mod argument;
pub mod attribute;
pub mod build;
pub mod code_type;
// pub mod debug_info;
pub mod evmla_data;
pub mod function;
pub mod global;
pub mod r#loop;
pub mod pointer;
pub mod solidity_data;
pub mod yul_data;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use inkwell::types::BasicType;
use inkwell::values::BasicValue;

use crate::optimizer::settings::Settings as OptimizerSettings;
use crate::optimizer::Optimizer;
use crate::polkavm::r#const::*;
use crate::polkavm::DebugConfig;
use crate::polkavm::Dependency;
use crate::target_machine::target::Target;
use crate::target_machine::TargetMachine;

use self::address_space::AddressSpace;
use self::attribute::Attribute;
use self::build::Build;
use self::code_type::CodeType;
// use self::debug_info::DebugInfo;
use self::evmla_data::EVMLAData;
use self::function::declaration::Declaration as FunctionDeclaration;
use self::function::intrinsics::Intrinsics;
use self::function::llvm_runtime::LLVMRuntime;
use self::function::r#return::Return as FunctionReturn;
use self::function::Function;
use self::global::Global;
use self::pointer::Pointer;
use self::r#loop::Loop;
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

    /// The project dependency manager. It can be any entity implementing the trait.
    /// The manager is used to get information about contracts and their dependencies during
    /// the multi-threaded compilation process.
    dependency_manager: Option<D>,
    /// Whether to append the metadata hash at the end of bytecode.
    include_metadata_hash: bool,
    /// The debug info of the current module.
    // debug_info: DebugInfo<'ctx>,
    /// The debug configuration telling whether to dump the needed IRs.
    debug_config: Option<DebugConfig>,

    /// The Solidity data.
    solidity_data: Option<SolidityData>,
    /// The Yul data.
    yul_data: Option<YulData>,
    /// The EVM legacy assembly data.
    evmla_data: Option<EVMLAData<'ctx>>,
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

    /// The PolkaVM minimum stack size.
    const POLKAVM_STACK_SIZE: u32 = 0x4000;

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

        for import in runtime_api::imports::IMPORTS {
            module
                .get_function(import)
                .expect("should be declared")
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
        let exports = revive_runtime_api::immutable_data::module(self.llvm(), size);
        self.module.link_in_module(exports).map_err(|error| {
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

    /// Initializes a new LLVM context.
    pub fn new(
        llvm: &'ctx inkwell::context::Context,
        module: inkwell::module::Module<'ctx>,
        optimizer: Optimizer,
        dependency_manager: Option<D>,
        include_metadata_hash: bool,
        debug_config: Option<DebugConfig>,
    ) -> Self {
        Self::link_stdlib_module(llvm, &module);
        Self::link_polkavm_imports(llvm, &module);
        Self::set_polkavm_stack_size(llvm, &module, Self::POLKAVM_STACK_SIZE);
        Self::set_module_flags(llvm, &module);

        let intrinsics = Intrinsics::new(llvm, &module);
        let llvm_runtime = LLVMRuntime::new(llvm, &module, &optimizer);

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

            dependency_manager,
            include_metadata_hash,
            // debug_info,
            debug_config,

            solidity_data: None,
            yul_data: None,
            evmla_data: None,
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
        target_machine.set_target_data(self.module());

        if let Some(ref debug_config) = self.debug_config {
            debug_config.dump_llvm_ir_unoptimized(contract_path, self.module())?;
        }
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
        if let Some(ref debug_config) = self.debug_config {
            debug_config.dump_llvm_ir_optimized(contract_path, self.module())?;
        }
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

        let bytecode = revive_linker::link(buffer.as_slice())?;

        let build = match crate::polkavm::build_assembly_text(
            contract_path,
            &bytecode,
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

        let entry_block = self.llvm.append_basic_block(value, "entry");
        let return_block = self.llvm.append_basic_block(value, "return");

        let r#return = match return_values_length {
            0 => FunctionReturn::none(),
            1 => {
                self.set_basic_block(entry_block);
                let pointer = self.build_alloca(self.word_type(), "return_pointer");
                FunctionReturn::primitive(pointer)
            }
            size if name.starts_with(Function::ZKSYNC_NEAR_CALL_ABI_PREFIX) => {
                let first_argument = value.get_first_param().expect("Always exists");
                let r#type = self.structure_type(vec![self.word_type(); size].as_slice());
                let pointer = first_argument.into_pointer_value();
                FunctionReturn::compound(Pointer::new(r#type, AddressSpace::Stack, pointer), size)
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

    /// Sets the current active function.
    pub fn set_current_function(&mut self, name: &str) -> anyhow::Result<()> {
        let function = self.functions.get(name).cloned().ok_or_else(|| {
            anyhow::anyhow!("Failed to activate an undeclared function `{}`", name)
        })?;
        self.current_function = Some(function);
        Ok(())
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

    /// Returns the debug config reference.
    pub fn debug_config(&self) -> Option<&DebugConfig> {
        self.debug_config.as_ref()
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
                let heap_pointer = self.build_heap_gep(
                    self.builder().build_ptr_to_int(
                        pointer.value,
                        self.xlen_type(),
                        "offset_ptrtoint",
                    )?,
                    pointer
                        .r#type
                        .size_of()
                        .expect("should be IntValue")
                        .const_truncate(self.xlen_type()),
                )?;

                let value = self
                    .builder()
                    .build_load(pointer.r#type, heap_pointer.value, name)?;
                self.basic_block()
                    .get_last_instruction()
                    .expect("Always exists")
                    .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
                    .expect("Alignment is valid");

                self.build_byte_swap(value)
            }
            AddressSpace::Storage | AddressSpace::TransientStorage => {
                let storage_key_value = self.builder().build_ptr_to_int(
                    pointer.value,
                    self.word_type(),
                    "storage_ptr_to_int",
                )?;
                let storage_key_pointer = self.build_alloca(self.word_type(), "storage_key");
                let storage_key_pointer_casted = self.builder().build_ptr_to_int(
                    storage_key_pointer.value,
                    self.xlen_type(),
                    "storage_key_pointer_casted",
                )?;
                self.builder()
                    .build_store(storage_key_pointer.value, storage_key_value)?;

                let storage_value_pointer =
                    self.build_alloca(self.word_type(), "storage_value_pointer");
                let storage_value_length_pointer =
                    self.build_alloca(self.xlen_type(), "storage_value_length_pointer");
                self.build_store(
                    storage_value_length_pointer,
                    self.word_const(revive_common::BIT_LENGTH_WORD as u64),
                )?;

                let transient = pointer.address_space == AddressSpace::TransientStorage;

                self.build_runtime_call(
                    runtime_api::imports::GET_STORAGE,
                    &[
                        self.xlen_type().const_int(transient as u64, false).into(),
                        storage_key_pointer_casted.into(),
                        self.xlen_type().const_all_ones().into(),
                        storage_value_pointer.to_int(self).into(),
                        storage_value_length_pointer.to_int(self).into(),
                    ],
                );

                // We do not to check the return value.
                // Solidity assumes infallible SLOAD.
                // If a key doesn't exist the "zero" value is returned.

                self.build_load(storage_value_pointer, "storage_value_load")
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
                let heap_pointer = self.build_heap_gep(
                    self.builder().build_ptr_to_int(
                        pointer.value,
                        self.xlen_type(),
                        "offset_ptrtoint",
                    )?,
                    value
                        .as_basic_value_enum()
                        .get_type()
                        .size_of()
                        .expect("should be IntValue")
                        .const_truncate(self.xlen_type()),
                )?;

                let value = value.as_basic_value_enum();
                let value = match value.get_type().into_int_type().get_bit_width() as usize {
                    revive_common::BIT_LENGTH_WORD => self.build_byte_swap(value)?,
                    revive_common::BIT_LENGTH_BYTE => value,
                    _ => unreachable!("Only word and byte sized values can be stored on EVM heap"),
                };

                self.builder
                    .build_store(heap_pointer.value, value)?
                    .set_alignment(revive_common::BYTE_LENGTH_BYTE as u32)
                    .expect("Alignment is valid");
            }
            AddressSpace::Storage | AddressSpace::TransientStorage => {
                assert_eq!(
                    value.as_basic_value_enum().get_type(),
                    self.word_type().as_basic_type_enum()
                );

                let storage_key_value = self.builder().build_ptr_to_int(
                    pointer.value,
                    self.word_type(),
                    "storage_ptr_to_int",
                )?;
                let storage_key_pointer = self.build_alloca(self.word_type(), "storage_key");
                let storage_key_pointer_casted = self.builder().build_ptr_to_int(
                    storage_key_pointer.value,
                    self.xlen_type(),
                    "storage_key_pointer_casted",
                )?;

                let storage_value_pointer = self.build_alloca(self.word_type(), "storage_value");
                let storage_value_pointer_casted = self.builder().build_ptr_to_int(
                    storage_value_pointer.value,
                    self.xlen_type(),
                    "storage_value_pointer_casted",
                )?;

                self.builder()
                    .build_store(storage_key_pointer.value, storage_key_value)?;
                self.builder()
                    .build_store(storage_value_pointer.value, value)?;

                let transient = pointer.address_space == AddressSpace::TransientStorage;

                self.build_runtime_call(
                    runtime_api::imports::SET_STORAGE,
                    &[
                        self.xlen_type().const_int(transient as u64, false).into(),
                        storage_key_pointer_casted.into(),
                        self.xlen_type().const_all_ones().into(),
                        storage_value_pointer_casted.into(),
                        self.integer_const(crate::polkavm::XLEN, 32).into(),
                    ],
                );
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
        assert_ne!(pointer.address_space, AddressSpace::Storage);
        assert_ne!(pointer.address_space, AddressSpace::TransientStorage);

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
                &format!("runtime API call {name}"),
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
        let offset_truncated = self.safe_truncate_int_to_xlen(offset)?;
        let length_truncated = self.safe_truncate_int_to_xlen(length)?;
        let offset_into_heap = self.build_heap_gep(offset_truncated, length_truncated)?;

        let length_pointer = self.safe_truncate_int_to_xlen(length)?;
        let offset_pointer = self.builder().build_ptr_to_int(
            offset_into_heap.value,
            self.xlen_type(),
            "return_data_ptr_to_int",
        )?;

        self.build_runtime_call(
            runtime_api::imports::RETURN,
            &[flags.into(), offset_pointer.into(), length_pointer.into()],
        );
        self.build_unreachable();

        Ok(())
    }

    /// Truncate a memory offset to register size, trapping if it doesn't fit.
    /// Pointers are represented as opaque 256 bit integer values in EVM.
    /// In practice, they should never exceed a register sized bit value.
    /// However, we still protect against this possibility here. Heap index
    /// offsets are generally untrusted and potentially represent valid
    /// (but wrong) pointers when truncated.
    ///
    /// TODO: Splitting up into a dedicated function
    /// could potentially decrease code sizes (LLVM can still decide to inline).
    /// However, passing i256 parameters is counter productive and
    /// I've found that splitting it up actualy increases code size.
    /// Should be reviewed after 64bit support.
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

        let truncated =
            self.builder()
                .build_int_truncate(value, self.xlen_type(), "offset_truncated")?;
        let extended =
            self.builder()
                .build_int_z_extend(truncated, self.word_type(), "offset_extended")?;
        let is_overflow = self.builder().build_int_compare(
            inkwell::IntPredicate::NE,
            value,
            extended,
            "compare_truncated_extended",
        )?;

        let block_continue = self.append_basic_block("offset_pointer_ok");
        let block_trap = self.append_basic_block("offset_pointer_overflow");
        self.build_conditional_branch(is_overflow, block_trap, block_continue)?;

        self.set_basic_block(block_trap);
        self.build_call(self.intrinsics().trap, &[], "invalid_trap");
        self.build_unreachable();

        self.set_basic_block(block_continue);
        Ok(truncated)
    }

    /// Build a call to PolkaVM `sbrk` for extending the heap by `size`.
    pub fn build_sbrk(
        &self,
        size: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<inkwell::values::PointerValue<'ctx>> {
        Ok(self
            .builder()
            .build_call(
                self.runtime_api_method(runtime_api::SBRK),
                &[size.into()],
                "call_sbrk",
            )?
            .try_as_basic_value()
            .left()
            .expect("sbrk returns a pointer")
            .into_pointer_value())
    }

    /// Call PolkaVM `sbrk` for extending the heap by `size`,
    /// trapping the contract if the call failed.
    /// Returns the end of memory pointer.
    pub fn build_heap_alloc(
        &self,
        size: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<inkwell::values::PointerValue<'ctx>> {
        let end_of_memory = self.build_sbrk(size)?;
        let return_is_nil = self.builder().build_int_compare(
            inkwell::IntPredicate::EQ,
            end_of_memory,
            self.llvm().ptr_type(Default::default()).const_null(),
            "compare_end_of_memory_nil",
        )?;

        let continue_block = self.append_basic_block("sbrk_not_nil");
        let trap_block = self.append_basic_block("sbrk_nil");
        self.build_conditional_branch(return_is_nil, trap_block, continue_block)?;

        self.set_basic_block(trap_block);
        self.build_call(self.intrinsics().trap, &[], "invalid_trap");
        self.build_unreachable();

        self.set_basic_block(continue_block);

        Ok(end_of_memory)
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

        let heap_start = self
            .get_global(crate::polkavm::GLOBAL_HEAP_MEMORY_POINTER)?
            .value
            .as_pointer_value();
        let heap_end = self.build_sbrk(self.integer_const(crate::polkavm::XLEN, 0))?;
        let value_end = self.build_gep(
            Pointer::new(self.byte_type(), AddressSpace::Stack, heap_start),
            &[self.builder().build_int_nuw_add(offset, length, "end")?],
            self.byte_type(),
            "heap_end_gep",
        );
        let is_out_of_bounds = self.builder().build_int_compare(
            inkwell::IntPredicate::UGT,
            value_end.value,
            heap_end,
            "is_value_overflowing_heap",
        )?;

        let out_of_bounds_block = self.append_basic_block("heap_offset_out_of_bounds");
        let heap_offset_block = self.append_basic_block("build_heap_pointer");
        self.build_conditional_branch(is_out_of_bounds, out_of_bounds_block, heap_offset_block)?;

        self.set_basic_block(out_of_bounds_block);
        let size = self.builder().build_int_nuw_sub(
            self.builder()
                .build_ptr_to_int(value_end.value, self.xlen_type(), "value_end")?,
            self.builder()
                .build_ptr_to_int(heap_end, self.xlen_type(), "heap_end")?,
            "heap_alloc_size",
        )?;
        self.build_heap_alloc(size)?;
        self.build_unconditional_branch(heap_offset_block);

        self.set_basic_block(heap_offset_block);
        Ok(self.build_gep(
            Pointer::new(self.byte_type(), AddressSpace::Stack, heap_start),
            &[offset],
            self.byte_type(),
            "heap_offset_via_gep",
        ))
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

    /// Returns the register witdh sized type.
    pub fn xlen_type(&self) -> inkwell::types::IntType<'ctx> {
        self.llvm.custom_width_int_type(crate::polkavm::XLEN as u32)
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
                if function == self.llvm_runtime().sha3 {
                    call_site_value.add_attribute(
                        inkwell::attributes::AttributeLoc::Param(index as u32),
                        self.llvm
                            .create_enum_attribute(Attribute::ReadOnly as u32, 0),
                    );
                }
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

    /// Sets the EVM legacy assembly data.
    pub fn set_evmla_data(&mut self, data: EVMLAData<'ctx>) {
        self.evmla_data = Some(data);
    }

    /// Returns the EVM legacy assembly data reference.
    /// # Panics
    /// If the EVM data has not been initialized.
    pub fn evmla(&self) -> &EVMLAData<'ctx> {
        self.evmla_data
            .as_ref()
            .expect("The EVMLA data must have been initialized")
    }

    /// Returns the EVM legacy assembly data mutable reference.
    /// # Panics
    /// If the EVM data has not been initialized.
    pub fn evmla_mut(&mut self) -> &mut EVMLAData<'ctx> {
        self.evmla_data
            .as_mut()
            .expect("The EVMLA data must have been initialized")
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
}
