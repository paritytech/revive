//!
//! The LLVM IR generator context.
//!

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
pub mod vyper_data;
pub mod yul_data;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use inkwell::types::BasicType;
use inkwell::values::BasicValue;

use crate::eravm::DebugConfig;
use crate::eravm::Dependency;
use crate::optimizer::settings::Settings as OptimizerSettings;
use crate::optimizer::Optimizer;
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
use self::vyper_data::VyperData;
use self::yul_data::YulData;

///
/// The LLVM IR generator context.
///
/// It is a not-so-big god-like object glueing all the compilers' complexity and act as an adapter
/// and a superstructure over the inner `inkwell` LLVM context.
///
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
    /// The Vyper data.
    vyper_data: Option<VyperData>,
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

    /// Link in the PolkaVM guest module, containing imported and exported functions,
    /// and marking them as external (they need to be relocatable as too).
    fn link_polkavm_guest_module(
        llvm: &'ctx inkwell::context::Context,
        module: &inkwell::module::Module<'ctx>,
    ) {
        module
            .link_in_module(pallet_contracts_pvm_llapi::module(llvm, "polkavm_guest").unwrap())
            .expect("the PolkaVM guest API module should be linkable");

        let call_function = module.get_function("call").unwrap();
        call_function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            llvm.create_enum_attribute(Attribute::NoReturn as u32, 0),
        );
        assert!(call_function.get_first_basic_block().is_none());

        let deploy_function = module.get_function("deploy").unwrap();
        deploy_function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            llvm.create_enum_attribute(Attribute::NoReturn as u32, 0),
        );
        assert!(deploy_function.get_first_basic_block().is_none());

        // TODO: Factor out a list
        // Also should be prefixed with double underscores
        for name in ["seal_return", "input", "set_storage", "get_storage"] {
            let runtime_api_function = module.get_function(name).expect("should be declared");
            runtime_api_function.set_linkage(inkwell::module::Linkage::External);
        }
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

    ///
    /// Initializes a new LLVM context.
    ///
    pub fn new(
        llvm: &'ctx inkwell::context::Context,
        module: inkwell::module::Module<'ctx>,
        optimizer: Optimizer,
        dependency_manager: Option<D>,
        include_metadata_hash: bool,
        debug_config: Option<DebugConfig>,
    ) -> Self {
        Self::link_stdlib_module(llvm, &module);
        Self::link_polkavm_guest_module(llvm, &module);
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
            vyper_data: None,
        }
    }

    ///
    /// Builds the LLVM IR module, returning the build artifacts.
    ///
    pub fn build(
        mut self,
        contract_path: &str,
        metadata_hash: Option<[u8; era_compiler_common::BYTE_LENGTH_FIELD]>,
    ) -> anyhow::Result<Build> {
        let module_clone = self.module.clone();

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

        let assembly_text = revive_linker::link(buffer.as_slice()).map(hex::encode)?;

        let build = match crate::eravm::build_assembly_text(
            contract_path,
            assembly_text.as_str(),
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

    ///
    /// Verifies the current LLVM IR module.
    ///
    pub fn verify(&self) -> anyhow::Result<()> {
        self.module()
            .verify()
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    ///
    /// Returns the inner LLVM context.
    ///
    pub fn llvm(&self) -> &'ctx inkwell::context::Context {
        self.llvm
    }

    ///
    /// Returns the LLVM IR builder.
    ///
    pub fn builder(&self) -> &inkwell::builder::Builder<'ctx> {
        &self.builder
    }

    ///
    /// Returns the current LLVM IR module reference.
    ///
    pub fn module(&self) -> &inkwell::module::Module<'ctx> {
        &self.module
    }

    ///
    /// Sets the current code type (deploy or runtime).
    ///
    pub fn set_code_type(&mut self, code_type: CodeType) {
        self.code_type = Some(code_type);
    }

    ///
    /// Returns the current code type (deploy or runtime).
    ///
    pub fn code_type(&self) -> Option<CodeType> {
        self.code_type.to_owned()
    }

    ///
    /// Returns the pointer to a global variable.
    ///
    pub fn get_global(&self, name: &str) -> anyhow::Result<Global<'ctx>> {
        match self.globals.get(name) {
            Some(global) => Ok(*global),
            None => anyhow::bail!("Global variable {} is not declared", name),
        }
    }

    ///
    /// Returns the value of a global variable.
    ///
    pub fn get_global_value(
        &self,
        name: &str,
    ) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
        let global = self.get_global(name)?;
        self.build_load(global.into(), name)
    }

    ///
    /// Sets the value to a global variable.
    ///
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

    ///
    /// Returns the LLVM intrinsics collection reference.
    ///
    pub fn intrinsics(&self) -> &Intrinsics<'ctx> {
        &self.intrinsics
    }

    ///
    /// Returns the LLVM runtime function collection reference.
    ///
    pub fn llvm_runtime(&self) -> &LLVMRuntime<'ctx> {
        &self.llvm_runtime
    }

    /// Declare a function already existing in the module.
    pub fn declare_extern_function(
        &mut self,
        name: &str,
    ) -> anyhow::Result<Rc<RefCell<Function<'ctx>>>> {
        let function = self.module().get_function(name).ok_or_else(|| {
            anyhow::anyhow!("Failed to activate an undeclared function `{}`", name)
        })?;

        let basic_block = self.llvm.append_basic_block(function, name);
        let declaration = FunctionDeclaration::new(
            self.function_type::<inkwell::types::BasicTypeEnum>(vec![], 0, false),
            function,
        );
        let function = Function::new(
            name.to_owned(),
            declaration,
            FunctionReturn::None,
            basic_block,
            basic_block,
        );
        Function::set_default_attributes(self.llvm, function.declaration(), &self.optimizer);

        let function = Rc::new(RefCell::new(function));
        self.functions.insert(name.to_string(), function.clone());

        Ok(function)
    }

    ///
    /// Appends a function to the current module.
    ///
    pub fn add_function(
        &mut self,
        name: &str,
        r#type: inkwell::types::FunctionType<'ctx>,
        return_values_length: usize,
        mut linkage: Option<inkwell::module::Linkage>,
    ) -> anyhow::Result<Rc<RefCell<Function<'ctx>>>> {
        if Function::is_near_call_abi(name) && self.is_system_mode() {
            linkage = Some(inkwell::module::Linkage::External);
        }

        let value = self.module().add_function(name, r#type, linkage);

        let entry_block = self.llvm.append_basic_block(value, "entry");
        let return_block = self.llvm.append_basic_block(value, "return");

        value.set_personality_function(self.llvm_runtime.personality.value);

        let r#return = match return_values_length {
            0 => FunctionReturn::none(),
            1 => {
                self.set_basic_block(entry_block);
                let pointer = self.build_alloca(self.field_type(), "return_pointer");
                FunctionReturn::primitive(pointer)
            }
            size if name.starts_with(Function::ZKSYNC_NEAR_CALL_ABI_PREFIX) => {
                let first_argument = value.get_first_param().expect("Always exists");
                let r#type = self.structure_type(vec![self.field_type(); size].as_slice());
                let pointer = first_argument.into_pointer_value();
                FunctionReturn::compound(Pointer::new(r#type, AddressSpace::Stack, pointer), size)
            }
            size => {
                self.set_basic_block(entry_block);
                let pointer = self.build_alloca(
                    self.structure_type(
                        vec![self.field_type().as_basic_type_enum(); size].as_slice(),
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
        if Function::is_near_call_abi(function.name()) && self.is_system_mode() {
            Function::set_exception_handler_attributes(self.llvm, function.declaration());
        }

        let function = Rc::new(RefCell::new(function));
        self.functions.insert(name.to_string(), function.clone());

        Ok(function)
    }

    ///
    /// Returns a shared reference to the specified function.
    ///
    pub fn get_function(&self, name: &str) -> Option<Rc<RefCell<Function<'ctx>>>> {
        self.functions.get(name).cloned()
    }

    ///
    /// Returns a shared reference to the current active function.
    ///
    pub fn current_function(&self) -> Rc<RefCell<Function<'ctx>>> {
        self.current_function
            .clone()
            .expect("Must be declared before use")
    }

    ///
    /// Sets the current active function.
    ///
    pub fn set_current_function(&mut self, name: &str) -> anyhow::Result<()> {
        let function = self.functions.get(name).cloned().ok_or_else(|| {
            anyhow::anyhow!("Failed to activate an undeclared function `{}`", name)
        })?;
        self.current_function = Some(function);
        Ok(())
    }

    ///
    /// Pushes a new loop context to the stack.
    ///
    pub fn push_loop(
        &mut self,
        body_block: inkwell::basic_block::BasicBlock<'ctx>,
        continue_block: inkwell::basic_block::BasicBlock<'ctx>,
        join_block: inkwell::basic_block::BasicBlock<'ctx>,
    ) {
        self.loop_stack
            .push(Loop::new(body_block, continue_block, join_block));
    }

    ///
    /// Pops the current loop context from the stack.
    ///
    pub fn pop_loop(&mut self) {
        self.loop_stack.pop();
    }

    ///
    /// Returns the current loop context.
    ///
    pub fn r#loop(&self) -> &Loop<'ctx> {
        self.loop_stack
            .last()
            .expect("The current context is not in a loop")
    }

    ///
    /// Compiles a contract dependency, if the dependency manager is set.
    ///
    pub fn compile_dependency(&mut self, name: &str) -> anyhow::Result<String> {
        if let Some(vyper_data) = self.vyper_data.as_mut() {
            vyper_data.set_is_forwarder_used();
        }
        self.dependency_manager
            .to_owned()
            .ok_or_else(|| anyhow::anyhow!("The dependency manager is unset"))
            .and_then(|manager| {
                Dependency::compile(
                    manager,
                    name,
                    self.optimizer.settings().to_owned(),
                    self.yul_data
                        .as_ref()
                        .map(|data| data.is_system_mode())
                        .unwrap_or_default(),
                    self.include_metadata_hash,
                    self.debug_config.clone(),
                )
            })
    }

    ///
    /// Gets a full contract_path from the dependency manager.
    ///
    pub fn resolve_path(&self, identifier: &str) -> anyhow::Result<String> {
        self.dependency_manager
            .to_owned()
            .ok_or_else(|| anyhow::anyhow!("The dependency manager is unset"))
            .and_then(|manager| {
                let full_path = manager.resolve_path(identifier)?;
                Ok(full_path)
            })
    }

    ///
    /// Gets a deployed library address from the dependency manager.
    ///
    pub fn resolve_library(&self, path: &str) -> anyhow::Result<inkwell::values::IntValue<'ctx>> {
        self.dependency_manager
            .to_owned()
            .ok_or_else(|| anyhow::anyhow!("The dependency manager is unset"))
            .and_then(|manager| {
                let address = manager.resolve_library(path)?;
                let address = self.field_const_str_hex(address.as_str());
                Ok(address)
            })
    }

    ///
    /// Extracts the dependency manager.
    ///
    pub fn take_dependency_manager(&mut self) -> D {
        self.dependency_manager
            .take()
            .expect("The dependency manager is unset")
    }

    ///
    /// Returns the debug config reference.
    ///
    pub fn debug_config(&self) -> Option<&DebugConfig> {
        self.debug_config.as_ref()
    }

    ///
    /// Appends a new basic block to the current function.
    ///
    pub fn append_basic_block(&self, name: &str) -> inkwell::basic_block::BasicBlock<'ctx> {
        self.llvm
            .append_basic_block(self.current_function().borrow().declaration().value, name)
    }

    ///
    /// Sets the current basic block.
    ///
    pub fn set_basic_block(&self, block: inkwell::basic_block::BasicBlock<'ctx>) {
        self.builder.position_at_end(block);
    }

    ///
    /// Returns the current basic block.
    ///
    pub fn basic_block(&self) -> inkwell::basic_block::BasicBlock<'ctx> {
        self.builder.get_insert_block().expect("Always exists")
    }

    ///
    /// Builds a stack allocation instruction.
    ///
    /// Sets the alignment to 256 bits.
    ///
    pub fn build_alloca<T: BasicType<'ctx> + Clone + Copy>(
        &self,
        r#type: T,
        name: &str,
    ) -> Pointer<'ctx> {
        let pointer = self.builder.build_alloca(r#type, name).unwrap();
        self.basic_block()
            .get_last_instruction()
            .expect("Always exists")
            .set_alignment(era_compiler_common::BYTE_LENGTH_FIELD as u32)
            .expect("Alignment is valid");
        Pointer::new(r#type, AddressSpace::Stack, pointer)
    }

    ///
    /// Builds a stack load instruction.
    ///
    /// Sets the alignment to 256 bits for the stack and 1 bit for the heap, parent, and child.
    ///
    pub fn build_load(
        &self,
        pointer: Pointer<'ctx>,
        name: &str,
    ) -> anyhow::Result<inkwell::values::BasicValueEnum<'ctx>> {
        match pointer.address_space {
            AddressSpace::Heap => {
                let heap_pointer = self.get_global(crate::eravm::GLOBAL_HEAP_MEMORY_POINTER)?;

                // TODO: Ensure safe casts somehow
                let offset = self.builder().build_ptr_to_int(
                    pointer.value,
                    self.integer_type(32),
                    "offset_ptrtoint",
                )?;
                let pointer_value = unsafe {
                    self.builder
                        .build_gep(
                            self.byte_type(),
                            heap_pointer.value.as_pointer_value(),
                            &[offset],
                            "heap_offset_via_gep",
                        )
                        .unwrap()
                };
                let value = self
                    .builder()
                    .build_load(pointer.r#type, pointer_value, name)?;
                self.basic_block()
                    .get_last_instruction()
                    .expect("Always exists")
                    .set_alignment(era_compiler_common::BYTE_LENGTH_BYTE as u32)
                    .expect("Alignment is valid");

                Ok(self.build_byte_swap(value))
            }
            AddressSpace::TransientStorage => todo!(),
            AddressSpace::Storage => {
                let storage_key_value = self.builder().build_ptr_to_int(
                    pointer.value,
                    self.field_type(),
                    "storage_ptr_to_int",
                )?;
                let storage_key_pointer =
                    self.build_alloca(storage_key_value.get_type(), "storage_key");
                let storage_key_pointer_casted = self.builder().build_ptr_to_int(
                    storage_key_pointer.value,
                    self.integer_type(32),
                    "storage_key_pointer_casted",
                )?;

                let storage_value_pointer = self.build_alloca(self.field_type(), "storage_value");
                let storage_value_pointer_casted = self.builder().build_ptr_to_int(
                    storage_value_pointer.value,
                    self.integer_type(32),
                    "storage_value_pointer_casted",
                )?;

                let storage_value_length_pointer =
                    self.build_alloca(self.integer_type(32), "out_len_ptr");
                let storage_value_length_pointer_casted = self.builder().build_ptr_to_int(
                    storage_value_length_pointer.value,
                    self.integer_type(32),
                    "storage_value_length_pointer_cast",
                )?;

                self.builder()
                    .build_store(storage_key_pointer.value, storage_key_value)?;
                self.builder().build_store(
                    storage_value_length_pointer.value,
                    self.integer_const(32, era_compiler_common::BIT_LENGTH_FIELD as u64),
                )?;

                let runtime_api = self
                    .module()
                    .get_function("get_storage")
                    .expect("should be declared");
                let arguments = &[
                    storage_key_pointer_casted.into(),
                    self.integer_const(32, 32).into(),
                    storage_value_pointer_casted.into(),
                    storage_value_length_pointer_casted.into(),
                ];
                let _ = self
                    .builder()
                    .build_call(runtime_api, arguments, "call_seal_get_storage")?
                    .try_as_basic_value()
                    .left()
                    .expect("should not be a void function type");
                // TODO check return value

                self.build_load(storage_value_pointer, "storage_value_load")
                    .map(|value| self.build_byte_swap(value))
            }
            AddressSpace::Code | AddressSpace::HeapAuxiliary => todo!(),
            AddressSpace::Generic => Ok(self.build_byte_swap(self.build_load(
                pointer.address_space_cast(self, AddressSpace::Stack, &format!("{}_cast", name))?,
                name,
            )?)),
            AddressSpace::Stack => {
                let value = self
                    .builder()
                    .build_load(pointer.r#type, pointer.value, name)?;

                let alignment = if AddressSpace::Stack == pointer.address_space {
                    era_compiler_common::BYTE_LENGTH_FIELD
                } else {
                    era_compiler_common::BYTE_LENGTH_BYTE
                };

                self.basic_block()
                    .get_last_instruction()
                    .expect("Always exists")
                    .set_alignment(alignment as u32)
                    .expect("Alignment is valid");
                Ok(value)
            }
        }
    }

    ///
    /// Builds a stack store instruction.
    ///
    /// Sets the alignment to 256 bits for the stack and 1 bit for the heap, parent, and child.
    ///
    pub fn build_store<V>(&self, pointer: Pointer<'ctx>, value: V) -> anyhow::Result<()>
    where
        V: BasicValue<'ctx>,
    {
        match pointer.address_space {
            AddressSpace::Heap => {
                let heap_pointer = self
                    .get_global(crate::eravm::GLOBAL_HEAP_MEMORY_POINTER)
                    .unwrap();

                // TODO: Ensure safe casts somehow
                let offset = self.builder().build_ptr_to_int(
                    pointer.value,
                    self.integer_type(32),
                    "offset_ptrtoint",
                )?;
                let pointer_value = unsafe {
                    self.builder()
                        .build_gep(
                            self.byte_type(),
                            heap_pointer.value.as_pointer_value(),
                            &[offset],
                            "heap_offset_via_gep",
                        )
                        .unwrap()
                };
                let value = self.build_byte_swap(value.as_basic_value_enum());

                let instruction = self.builder.build_store(pointer_value, value).unwrap();
                instruction
                    .set_alignment(era_compiler_common::BYTE_LENGTH_BYTE as u32)
                    .expect("Alignment is valid");
            }
            AddressSpace::TransientStorage => todo!(),
            AddressSpace::Storage => {
                // TODO: Tests, factor out into dedicated functions

                let storage_key_value = self.builder().build_ptr_to_int(
                    pointer.value,
                    self.field_type(),
                    "storage_ptr_to_int",
                )?;
                let storage_key_pointer =
                    self.build_alloca(storage_key_value.get_type(), "storage_key");

                let storage_value_value = self
                    .build_byte_swap(value.as_basic_value_enum())
                    .into_int_value();
                let storage_value_pointer =
                    self.build_alloca(storage_value_value.get_type(), "storage_value");

                let storage_key_pointer_casted = self.builder().build_ptr_to_int(
                    storage_key_pointer.value,
                    self.integer_type(32),
                    "storage_key_pointer_casted",
                )?;
                let storage_value_pointer_casted = self.builder().build_ptr_to_int(
                    storage_value_pointer.value,
                    self.integer_type(32),
                    "storage_value_pointer_casted",
                )?;

                self.builder()
                    .build_store(storage_key_pointer.value, storage_key_value)?;
                self.builder()
                    .build_store(storage_value_pointer.value, storage_value_value)?;

                let runtime_api = self
                    .module()
                    .get_function("set_storage")
                    .expect("should be declared");
                let arguments = &[
                    storage_key_pointer_casted.into(),
                    self.integer_const(32, 32).into(),
                    storage_value_pointer_casted.into(),
                    self.integer_const(32, 32).into(),
                ];
                self.builder()
                    .build_call(runtime_api, arguments, "call_seal_set_storage")?;
            }
            AddressSpace::Code | AddressSpace::HeapAuxiliary => {}
            AddressSpace::Generic => self.build_store(
                pointer.address_space_cast(self, AddressSpace::Stack, "cast")?,
                self.build_byte_swap(value.as_basic_value_enum()),
            )?,
            AddressSpace::Stack => {
                let instruction = self.builder.build_store(pointer.value, value).unwrap();
                instruction
                    .set_alignment(era_compiler_common::BYTE_LENGTH_FIELD as u32)
                    .expect("Alignment is valid");
            }
        };

        Ok(())
        // let instruction = self.builder.build_store(pointer.value, value).unwrap();

        // let alignment = if AddressSpace::Stack == pointer.address_space {
        //     era_compiler_common::BYTE_LENGTH_FIELD
        // } else {
        //     era_compiler_common::BYTE_LENGTH_BYTE
        // };

        // instruction
        //     .set_alignment(alignment as u32)
        //     .expect("Alignment is valid");
    }

    /// Swap the endianness of an intvalue
    pub fn build_byte_swap(
        &self,
        value: inkwell::values::BasicValueEnum<'ctx>,
    ) -> inkwell::values::BasicValueEnum<'ctx> {
        self.build_call(self.intrinsics().byte_swap, &[value], "call_byte_swap")
            .expect("byte_swap should return a value")
    }

    ///
    /// Builds a GEP instruction.
    ///
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
        assert_ne!(pointer.address_space, AddressSpace::Heap);
        assert_ne!(pointer.address_space, AddressSpace::HeapAuxiliary);
        assert_ne!(pointer.address_space, AddressSpace::TransientStorage);
        assert_ne!(pointer.address_space, AddressSpace::Code);

        let value = unsafe {
            self.builder
                .build_gep(pointer.r#type, pointer.value, indexes, name)
                .unwrap()
        };
        Pointer::new(element_type, pointer.address_space, value)
    }

    ///
    /// Builds a conditional branch.
    ///
    /// Checks if there are no other terminators in the block.
    ///
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

    ///
    /// Builds an unconditional branch.
    ///
    /// Checks if there are no other terminators in the block.
    ///
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

    ///
    /// Builds a call.
    ///
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

    ///
    /// Builds an invoke.
    ///
    /// Is defaulted to a call if there is no global exception handler.
    ///
    pub fn build_invoke(
        &self,
        function: FunctionDeclaration<'ctx>,
        arguments: &[inkwell::values::BasicValueEnum<'ctx>],
        name: &str,
    ) -> Option<inkwell::values::BasicValueEnum<'ctx>> {
        if !self
            .functions
            .contains_key(Function::ZKSYNC_NEAR_CALL_ABI_EXCEPTION_HANDLER)
        {
            return self.build_call(function, arguments, name);
        }

        let return_pointer = if let Some(r#type) = function.r#type.get_return_type() {
            let pointer = self.build_alloca(r#type, "invoke_return_pointer");
            self.build_store(pointer, r#type.const_zero()).unwrap();
            Some(pointer)
        } else {
            None
        };

        let success_block = self.append_basic_block("invoke_success_block");
        let catch_block = self.append_basic_block("invoke_catch_block");
        let current_block = self.basic_block();

        self.set_basic_block(catch_block);
        let landing_pad_type = self.structure_type(&[
            self.byte_type()
                .ptr_type(AddressSpace::Stack.into())
                .as_basic_type_enum(),
            self.integer_type(era_compiler_common::BIT_LENGTH_X32)
                .as_basic_type_enum(),
        ]);
        self.builder
            .build_landing_pad(
                landing_pad_type,
                self.llvm_runtime.personality.value,
                &[self
                    .byte_type()
                    .ptr_type(AddressSpace::Stack.into())
                    .const_zero()
                    .as_basic_value_enum()],
                false,
                "invoke_catch_landing",
            )
            .unwrap();
        crate::eravm::utils::throw(self);

        self.set_basic_block(current_block);
        let call_site_value = self
            .builder
            .build_indirect_invoke(
                function.r#type,
                function.value.as_global_value().as_pointer_value(),
                arguments,
                success_block,
                catch_block,
                name,
            )
            .unwrap();
        self.modify_call_site_value(arguments, call_site_value, function);

        self.set_basic_block(success_block);
        if let (Some(return_pointer), Some(mut return_value)) =
            (return_pointer, call_site_value.try_as_basic_value().left())
        {
            if let Some(return_type) = function.r#type.get_return_type() {
                if return_type.is_pointer_type() {
                    return_value = self
                        .builder()
                        .build_int_to_ptr(
                            return_value.into_int_value(),
                            return_type.into_pointer_type(),
                            format!("{name}_invoke_return_pointer_casted").as_str(),
                        )
                        .unwrap()
                        .as_basic_value_enum();
                }
            }
            self.build_store(return_pointer, return_value).unwrap();
        }
        return_pointer.map(|pointer| self.build_load(pointer, "invoke_result").unwrap())
    }

    ///
    /// Builds an invoke of local call covered with an exception handler.
    ///
    /// Yul does not the exception handling, so the user can declare a special handling function
    /// called (see constant `ZKSYNC_NEAR_CALL_ABI_EXCEPTION_HANDLER`. If the enclosed function
    /// panics, the control flow will be transferred to the exception handler.
    ///
    pub fn build_invoke_near_call_abi(
        &self,
        _function: FunctionDeclaration<'ctx>,
        _arguments: Vec<inkwell::values::BasicValueEnum<'ctx>>,
        _name: &str,
    ) -> Option<inkwell::values::BasicValueEnum<'ctx>> {
        unimplemented!()
    }

    ///
    /// Builds a memory copy call.
    ///
    /// Sets the alignment to `1`, since all non-stack memory pages have such alignment.
    ///
    pub fn build_memcpy(
        &self,
        _function: FunctionDeclaration<'ctx>,
        destination: Pointer<'ctx>,
        source: Pointer<'ctx>,
        size: inkwell::values::IntValue<'ctx>,
        _name: &str,
    ) -> anyhow::Result<()> {
        let _ = self
            .builder()
            .build_memcpy(destination.value, 1, source.value, 1, size)?;

        Ok(())
    }

    ///
    /// Builds a memory copy call for the return data.
    ///
    /// Sets the output length to `min(output_length, return_data_size` and calls the default
    /// generic page memory copy builder.
    ///
    pub fn build_memcpy_return_data(
        &self,
        function: FunctionDeclaration<'ctx>,
        destination: Pointer<'ctx>,
        source: Pointer<'ctx>,
        size: inkwell::values::IntValue<'ctx>,
        name: &str,
    ) -> anyhow::Result<()> {
        let pointer_casted = self.builder.build_ptr_to_int(
            source.value,
            self.field_type(),
            format!("{name}_pointer_casted").as_str(),
        )?;
        let return_data_size_shifted = self.builder.build_right_shift(
            pointer_casted,
            self.field_const((era_compiler_common::BIT_LENGTH_X32 * 3) as u64),
            false,
            format!("{name}_return_data_size_shifted").as_str(),
        )?;
        let return_data_size_truncated = self.builder.build_and(
            return_data_size_shifted,
            self.field_const(u32::MAX as u64),
            format!("{name}_return_data_size_truncated").as_str(),
        )?;
        let is_return_data_size_lesser = self.builder.build_int_compare(
            inkwell::IntPredicate::ULT,
            return_data_size_truncated,
            size,
            format!("{name}_is_return_data_size_lesser").as_str(),
        )?;
        let min_size = self
            .builder
            .build_select(
                is_return_data_size_lesser,
                return_data_size_truncated,
                size,
                format!("{name}_min_size").as_str(),
            )?
            .into_int_value();

        self.build_memcpy(function, destination, source, min_size, name)?;

        Ok(())
    }

    ///
    /// Builds a return.
    ///
    /// Checks if there are no other terminators in the block.
    ///
    pub fn build_return(&self, value: Option<&dyn BasicValue<'ctx>>) {
        if self.basic_block().get_terminator().is_some() {
            return;
        }

        self.builder.build_return(value).unwrap();
    }

    ///
    /// Builds an unreachable.
    ///
    /// Checks if there are no other terminators in the block.
    ///
    pub fn build_unreachable(&self) {
        if self.basic_block().get_terminator().is_some() {
            return;
        }

        self.builder.build_unreachable().unwrap();
    }

    ///
    /// Builds a long contract exit sequence.
    ///
    /// The deploy code does not return the runtime code like in EVM. Instead, it returns some
    /// additional contract metadata, e.g. the array of immutables.
    /// The deploy code uses the auxiliary heap for the return, because otherwise it is not possible
    /// to allocate memory together with the Yul allocator safely.
    ///
    pub fn build_exit(
        &self,
        flags: inkwell::values::IntValue<'ctx>,
        offset: inkwell::values::IntValue<'ctx>,
        length: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<()> {
        // TODO:
        //let return_forward_mode = if self.code_type() == Some(CodeType::Deploy)
        //    && return_function == self.llvm_runtime().r#return
        //{
        //    zkevm_opcode_defs::RetForwardPageType::UseAuxHeap
        //} else {
        //    zkevm_opcode_defs::RetForwardPageType::UseHeap
        //};

        let heap_pointer = self.get_global(crate::eravm::GLOBAL_HEAP_MEMORY_POINTER)?;
        let offset_truncated = self.safe_truncate_int_to_i32(offset)?;
        let offset_into_heap = unsafe {
            self.builder().build_gep(
                self.byte_type(),
                heap_pointer.value.as_pointer_value(),
                &[offset_truncated],
                "heap_offset_via_gep",
            )
        }?;

        let length_pointer = self.safe_truncate_int_to_i32(length)?;
        let offset_pointer = self.builder().build_ptr_to_int(
            offset_into_heap,
            self.integer_type(32),
            "return_data_ptr_to_int",
        )?;

        self.builder().build_call(
            self.module().get_function("seal_return").unwrap(),
            &[flags.into(), offset_pointer.into(), length_pointer.into()],
            "seal_return",
        )?;
        self.build_unreachable();

        Ok(())
    }

    /// Truncate a memory offset into 32 bits, trapping if it doesn't fit.
    ///
    /// Pointers are represented as opaque 256 bit integer values in EVM.
    /// In practice, they should never exceed a 32 bit value. However, we
    /// still protect against this possibility here.
    pub fn safe_truncate_int_to_i32(
        &self,
        value: inkwell::values::IntValue<'ctx>,
    ) -> anyhow::Result<inkwell::values::IntValue<'ctx>> {
        let truncated = self.builder().build_int_truncate_or_bit_cast(
            value,
            self.integer_type(32),
            "offset_truncated",
        )?;
        let extended = self.builder().build_int_z_extend_or_bit_cast(
            truncated,
            self.field_type(),
            "offset_extended",
        )?;
        let is_overflow = self.builder().build_int_compare(
            inkwell::IntPredicate::NE,
            value,
            extended,
            "compare_truncated_extended",
        )?;

        let continue_block = self.append_basic_block("offset_pointer_ok");
        let trap_block = self.append_basic_block("offset_pointer_overflow");
        self.build_conditional_branch(is_overflow, trap_block, continue_block)?;

        self.set_basic_block(trap_block);
        self.build_call(self.intrinsics().trap, &[], "invalid_trap");
        self.build_unreachable();

        self.set_basic_block(continue_block);
        Ok(truncated)
    }

    ///
    /// Writes the ABI pointer to the global variable.
    ///
    pub fn write_abi_pointer(&mut self, pointer: Pointer<'ctx>, global_name: &str) {
        self.set_global(
            global_name,
            self.byte_type().ptr_type(AddressSpace::Generic.into()),
            AddressSpace::Stack,
            pointer.value,
        );
    }

    ///
    /// Writes the ABI data size to the global variable.
    ///
    pub fn write_abi_data_size(&mut self, _pointer: Pointer<'ctx>, _global_name: &str) {
        /*
        let abi_pointer_value = self
            .builder()
            .build_ptr_to_int(pointer.value, self.field_type(), "abi_pointer_value")
            .unwrap();
        let abi_pointer_value_shifted = self
            .builder()
        .build_right_shift(
            abi_pointer_value,
                self.field_const((era_compiler_common::BIT_LENGTH_X32 * 3) as u64),
                false,
                "abi_pointer_value_shifted",
            )
            .unwrap();
        let abi_length_value = self
            .builder()
            .build_and(
                abi_pointer_value_shifted,
                self.field_const(u32::MAX as u64),
                "abi_length_value",
            )
            .unwrap();
        self.set_global(
            global_name,
            self.field_type(),
            AddressSpace::Stack,
            abi_length_value,
        );
        */
    }

    ///
    /// Returns a boolean type constant.
    ///
    pub fn bool_const(&self, value: bool) -> inkwell::values::IntValue<'ctx> {
        self.bool_type().const_int(u64::from(value), false)
    }

    ///
    /// Returns an integer type constant.
    ///
    pub fn integer_const(&self, bit_length: usize, value: u64) -> inkwell::values::IntValue<'ctx> {
        self.integer_type(bit_length).const_int(value, false)
    }

    ///
    /// Returns a 256-bit field type constant.
    ///
    pub fn field_const(&self, value: u64) -> inkwell::values::IntValue<'ctx> {
        self.field_type().const_int(value, false)
    }

    ///
    /// Returns a 256-bit field type undefined value.
    ///
    pub fn field_undef(&self) -> inkwell::values::IntValue<'ctx> {
        self.field_type().get_undef()
    }

    ///
    /// Returns a field type constant from a decimal string.
    ///
    pub fn field_const_str_dec(&self, value: &str) -> inkwell::values::IntValue<'ctx> {
        self.field_type()
            .const_int_from_string(value, inkwell::types::StringRadix::Decimal)
            .unwrap_or_else(|| panic!("Invalid string constant `{value}`"))
    }

    ///
    /// Returns a field type constant from a hexadecimal string.
    ///
    pub fn field_const_str_hex(&self, value: &str) -> inkwell::values::IntValue<'ctx> {
        self.field_type()
            .const_int_from_string(
                value.strip_prefix("0x").unwrap_or(value),
                inkwell::types::StringRadix::Hexadecimal,
            )
            .unwrap_or_else(|| panic!("Invalid string constant `{value}`"))
    }

    ///
    /// Returns the void type.
    ///
    pub fn void_type(&self) -> inkwell::types::VoidType<'ctx> {
        self.llvm.void_type()
    }

    ///
    /// Returns the boolean type.
    ///
    pub fn bool_type(&self) -> inkwell::types::IntType<'ctx> {
        self.llvm.bool_type()
    }

    ///
    /// Returns the default byte type.
    ///
    pub fn byte_type(&self) -> inkwell::types::IntType<'ctx> {
        self.llvm
            .custom_width_int_type(era_compiler_common::BIT_LENGTH_BYTE as u32)
    }

    ///
    /// Returns the integer type of the specified bit-length.
    ///
    pub fn integer_type(&self, bit_length: usize) -> inkwell::types::IntType<'ctx> {
        self.llvm.custom_width_int_type(bit_length as u32)
    }

    ///
    /// Returns the default field type.
    ///
    pub fn field_type(&self) -> inkwell::types::IntType<'ctx> {
        self.llvm
            .custom_width_int_type(era_compiler_common::BIT_LENGTH_FIELD as u32)
    }

    ///
    /// Returns the array type with the specified length.
    ///
    pub fn array_type<T>(&self, element_type: T, length: usize) -> inkwell::types::ArrayType<'ctx>
    where
        T: BasicType<'ctx>,
    {
        element_type.array_type(length as u32)
    }

    ///
    /// Returns the structure type with specified fields.
    ///
    pub fn structure_type<T>(&self, field_types: &[T]) -> inkwell::types::StructType<'ctx>
    where
        T: BasicType<'ctx>,
    {
        let field_types: Vec<inkwell::types::BasicTypeEnum<'ctx>> =
            field_types.iter().map(T::as_basic_type_enum).collect();
        self.llvm.struct_type(field_types.as_slice(), false)
    }

    ///
    /// Returns a Yul function type with the specified arguments and number of return values.
    ///
    pub fn function_type<T>(
        &self,
        argument_types: Vec<T>,
        return_values_size: usize,
        is_near_call_abi: bool,
    ) -> inkwell::types::FunctionType<'ctx>
    where
        T: BasicType<'ctx>,
    {
        let mut argument_types: Vec<inkwell::types::BasicMetadataTypeEnum> = argument_types
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
            1 => self.field_type().fn_type(argument_types.as_slice(), false),
            size if is_near_call_abi && self.is_system_mode() => {
                let return_types: Vec<_> = vec![self.field_type().as_basic_type_enum(); size];
                let return_type = self
                    .structure_type(return_types.as_slice())
                    .ptr_type(AddressSpace::Stack.into());
                argument_types.insert(0, return_type.as_basic_type_enum().into());
                return_type.fn_type(argument_types.as_slice(), false)
            }
            size => self
                .structure_type(vec![self.field_type().as_basic_type_enum(); size].as_slice())
                .fn_type(argument_types.as_slice(), false),
        }
    }

    ///
    /// Modifies the call site value, setting the default attributes.
    ///
    /// The attributes only affect the LLVM optimizations.
    ///
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
                    era_compiler_common::BYTE_LENGTH_FIELD as u32,
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
                if function == self.llvm_runtime().mstore8 {
                    call_site_value.add_attribute(
                        inkwell::attributes::AttributeLoc::Param(index as u32),
                        self.llvm
                            .create_enum_attribute(Attribute::WriteOnly as u32, 0),
                    );
                }
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
                            (era_compiler_common::BIT_LENGTH_FIELD * 2) as u64,
                        ),
                    );
                    call_site_value.add_attribute(
                        inkwell::attributes::AttributeLoc::Return,
                        self.llvm.create_enum_attribute(
                            Attribute::Dereferenceable as u32,
                            (era_compiler_common::BIT_LENGTH_FIELD * 2) as u64,
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
                era_compiler_common::BYTE_LENGTH_FIELD as u32,
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

    ///
    /// Sets the Solidity data.
    ///
    pub fn set_solidity_data(&mut self, data: SolidityData) {
        self.solidity_data = Some(data);
    }

    ///
    /// Returns the Solidity data reference.
    ///
    /// # Panics
    /// If the Solidity data has not been initialized.
    ///
    pub fn solidity(&self) -> &SolidityData {
        self.solidity_data
            .as_ref()
            .expect("The Solidity data must have been initialized")
    }

    ///
    /// Returns the Solidity data mutable reference.
    ///
    /// # Panics
    /// If the Solidity data has not been initialized.
    ///
    pub fn solidity_mut(&mut self) -> &mut SolidityData {
        self.solidity_data
            .as_mut()
            .expect("The Solidity data must have been initialized")
    }

    ///
    /// Sets the Yul data.
    ///
    pub fn set_yul_data(&mut self, data: YulData) {
        self.yul_data = Some(data);
    }

    ///
    /// Returns the Yul data reference.
    ///
    /// # Panics
    /// If the Yul data has not been initialized.
    ///
    pub fn yul(&self) -> &YulData {
        self.yul_data
            .as_ref()
            .expect("The Yul data must have been initialized")
    }

    ///
    /// Returns the Yul data mutable reference.
    ///
    /// # Panics
    /// If the Yul data has not been initialized.
    ///
    pub fn yul_mut(&mut self) -> &mut YulData {
        self.yul_data
            .as_mut()
            .expect("The Yul data must have been initialized")
    }

    ///
    /// Sets the EVM legacy assembly data.
    ///
    pub fn set_evmla_data(&mut self, data: EVMLAData<'ctx>) {
        self.evmla_data = Some(data);
    }

    ///
    /// Returns the EVM legacy assembly data reference.
    ///
    /// # Panics
    /// If the EVM data has not been initialized.
    ///
    pub fn evmla(&self) -> &EVMLAData<'ctx> {
        self.evmla_data
            .as_ref()
            .expect("The EVMLA data must have been initialized")
    }

    ///
    /// Returns the EVM legacy assembly data mutable reference.
    ///
    /// # Panics
    /// If the EVM data has not been initialized.
    ///
    pub fn evmla_mut(&mut self) -> &mut EVMLAData<'ctx> {
        self.evmla_data
            .as_mut()
            .expect("The EVMLA data must have been initialized")
    }

    ///
    /// Sets the EVM legacy assembly data.
    ///
    pub fn set_vyper_data(&mut self, data: VyperData) {
        self.vyper_data = Some(data);
    }

    ///
    /// Returns the Vyper data reference.
    ///
    /// # Panics
    /// If the Vyper data has not been initialized.
    ///
    pub fn vyper(&self) -> &VyperData {
        self.vyper_data
            .as_ref()
            .expect("The Solidity data must have been initialized")
    }

    ///
    /// Returns the current number of immutables values in the contract.
    ///
    /// If the size is set manually, then it is returned. Otherwise, the number of elements in
    /// the identifier-to-offset mapping tree is returned.
    ///
    pub fn immutables_size(&self) -> anyhow::Result<usize> {
        if let Some(solidity) = self.solidity_data.as_ref() {
            Ok(solidity.immutables_size())
        } else if let Some(vyper) = self.vyper_data.as_ref() {
            Ok(vyper.immutables_size())
        } else {
            anyhow::bail!("The immutable size data is not available");
        }
    }

    ///
    /// Whether the system mode is enabled.
    ///
    pub fn is_system_mode(&self) -> bool {
        self.yul_data
            .as_ref()
            .map(|data| data.is_system_mode())
            .unwrap_or_default()
    }
}
