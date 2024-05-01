//! The LLVM IR generator function.

pub mod block;
pub mod declaration;
pub mod evmla_data;
pub mod intrinsics;
pub mod llvm_runtime;
pub mod r#return;
pub mod runtime;
pub mod vyper_data;
pub mod yul_data;

use std::collections::HashMap;

use crate::eravm::context::attribute::Attribute;
use crate::eravm::context::pointer::Pointer;
use crate::optimizer::settings::size_level::SizeLevel;
use crate::optimizer::Optimizer;

use self::declaration::Declaration;
use self::evmla_data::EVMLAData;
use self::r#return::Return;
use self::runtime::Runtime;
use self::vyper_data::VyperData;
use self::yul_data::YulData;

/// The LLVM IR generator function.
#[derive(Debug)]
pub struct Function<'ctx> {
    /// The high-level source code name.
    name: String,
    /// The LLVM function declaration.
    declaration: Declaration<'ctx>,
    /// The stack representation.
    stack: HashMap<String, Pointer<'ctx>>,
    /// The return value entity.
    r#return: Return<'ctx>,

    /// The entry block. Each LLVM IR functions must have an entry block.
    entry_block: inkwell::basic_block::BasicBlock<'ctx>,
    /// The return/leave block. LLVM IR functions may have multiple returning blocks, but it is
    /// more reasonable to have a single returning block and other high-level language returns
    /// jumping to it. This way it is easier to implement some additional checks and clean-ups
    /// before the returning.
    return_block: inkwell::basic_block::BasicBlock<'ctx>,

    /// The Yul compiler data.
    yul_data: Option<YulData>,
    /// The EVM legacy assembly compiler data.
    evmla_data: Option<EVMLAData<'ctx>>,
    /// The Vyper data.
    vyper_data: Option<VyperData>,
}

impl<'ctx> Function<'ctx> {
    /// The near call ABI function prefix.
    pub const ZKSYNC_NEAR_CALL_ABI_PREFIX: &'static str = "ZKSYNC_NEAR_CALL";

    /// The near call ABI exception handler name.
    pub const ZKSYNC_NEAR_CALL_ABI_EXCEPTION_HANDLER: &'static str = "ZKSYNC_CATCH_NEAR_CALL";

    /// The stack hashmap default capacity.
    const STACK_HASHMAP_INITIAL_CAPACITY: usize = 64;

    /// A shortcut constructor.
    pub fn new(
        name: String,
        declaration: Declaration<'ctx>,
        r#return: Return<'ctx>,

        entry_block: inkwell::basic_block::BasicBlock<'ctx>,
        return_block: inkwell::basic_block::BasicBlock<'ctx>,
    ) -> Self {
        Self {
            name,
            declaration,
            stack: HashMap::with_capacity(Self::STACK_HASHMAP_INITIAL_CAPACITY),
            r#return,

            entry_block,
            return_block,

            yul_data: None,
            evmla_data: None,
            vyper_data: None,
        }
    }

    /// Returns the function name reference.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Checks whether the function is defined outside of the front-end.
    pub fn is_name_external(name: &str) -> bool {
        name.starts_with("llvm.")
            || (name.starts_with("__")
                && name != Runtime::FUNCTION_ENTRY
                && name != Runtime::FUNCTION_DEPLOY_CODE
                && name != Runtime::FUNCTION_RUNTIME_CODE)
    }

    /// Checks whether the function is related to the near call ABI.
    pub fn is_near_call_abi(name: &str) -> bool {
        name.starts_with(Self::ZKSYNC_NEAR_CALL_ABI_PREFIX)
            || name == Self::ZKSYNC_NEAR_CALL_ABI_EXCEPTION_HANDLER
    }

    /// Returns the LLVM function declaration.
    pub fn declaration(&self) -> Declaration<'ctx> {
        self.declaration
    }

    /// Returns the N-th parameter of the function.
    pub fn get_nth_param(&self, index: usize) -> inkwell::values::BasicValueEnum<'ctx> {
        self.declaration()
            .value
            .get_nth_param(index as u32)
            .expect("Always exists")
    }

    /// Sets the memory writer function attributes.
    pub fn set_attributes(
        llvm: &'ctx inkwell::context::Context,
        declaration: Declaration<'ctx>,
        attributes: Vec<Attribute>,
        force: bool,
    ) {
        for attribute_kind in attributes.into_iter() {
            match attribute_kind {
                attribute_kind @ Attribute::AlwaysInline if force => {
                    let is_optimize_none_set = declaration
                        .value
                        .get_enum_attribute(
                            inkwell::attributes::AttributeLoc::Function,
                            Attribute::OptimizeNone as u32,
                        )
                        .is_some();
                    if !is_optimize_none_set {
                        declaration.value.remove_enum_attribute(
                            inkwell::attributes::AttributeLoc::Function,
                            Attribute::NoInline as u32,
                        );
                        declaration.value.add_attribute(
                            inkwell::attributes::AttributeLoc::Function,
                            llvm.create_enum_attribute(attribute_kind as u32, 0),
                        );
                    }
                }
                attribute_kind @ Attribute::NoInline if force => {
                    declaration.value.remove_enum_attribute(
                        inkwell::attributes::AttributeLoc::Function,
                        Attribute::AlwaysInline as u32,
                    );
                    declaration.value.add_attribute(
                        inkwell::attributes::AttributeLoc::Function,
                        llvm.create_enum_attribute(attribute_kind as u32, 0),
                    );
                }
                attribute_kind => declaration.value.add_attribute(
                    inkwell::attributes::AttributeLoc::Function,
                    llvm.create_enum_attribute(attribute_kind as u32, 0),
                ),
            }
        }
    }

    /// Remove specified attributes existing on the given declaration.
    pub fn remove_attributes(declaration: Declaration, attributes: &[Attribute]) {
        for attribute in attributes.iter().filter(|attribute| {
            declaration
                .value
                .get_enum_attribute(
                    inkwell::attributes::AttributeLoc::Function,
                    **attribute as u32,
                )
                .is_some()
        }) {
            declaration.value.remove_enum_attribute(
                inkwell::attributes::AttributeLoc::Function,
                *attribute as u32,
            );
        }
    }

    /// Sets the default attributes.
    /// The attributes only affect the LLVM optimizations.
    pub fn set_default_attributes(
        llvm: &'ctx inkwell::context::Context,
        declaration: Declaration<'ctx>,
        optimizer: &Optimizer,
    ) {
        if optimizer.settings().level_middle_end == inkwell::OptimizationLevel::None {
            Self::remove_attributes(
                declaration,
                &[Attribute::OptimizeForSize, Attribute::AlwaysInline],
            );
            Self::set_attributes(
                llvm,
                declaration,
                vec![Attribute::OptimizeNone, Attribute::NoInline],
                false,
            );
        } else if optimizer.settings().level_middle_end_size == SizeLevel::Z {
            Self::set_attributes(
                llvm,
                declaration,
                vec![Attribute::OptimizeForSize, Attribute::MinSize],
                false,
            );
        }

        Self::set_attributes(llvm, declaration, vec![Attribute::NoFree], false);
    }

    /// Sets the front-end runtime attributes.
    pub fn set_frontend_runtime_attributes(
        llvm: &'ctx inkwell::context::Context,
        declaration: Declaration<'ctx>,
        optimizer: &Optimizer,
    ) {
        if optimizer.settings().level_middle_end_size == SizeLevel::Z {
            Self::set_attributes(llvm, declaration, vec![Attribute::NoInline], false);
        }
    }

    /// Sets the exception handler attributes.
    pub fn set_exception_handler_attributes(
        llvm: &'ctx inkwell::context::Context,
        declaration: Declaration<'ctx>,
    ) {
        Self::set_attributes(llvm, declaration, vec![Attribute::NoInline], false);
    }

    /// Sets the CXA-throw attributes.
    pub fn set_cxa_throw_attributes(
        llvm: &'ctx inkwell::context::Context,
        declaration: Declaration<'ctx>,
    ) {
        Self::set_attributes(llvm, declaration, vec![Attribute::NoProfile], false);
    }

    /// Sets the pure function attributes.
    pub fn set_pure_function_attributes(
        llvm: &'ctx inkwell::context::Context,
        declaration: Declaration<'ctx>,
    ) {
        Self::set_attributes(
            llvm,
            declaration,
            vec![
                Attribute::MustProgress,
                Attribute::NoUnwind,
                // FIXME: LLVM complains about ReadNone being not valid for fns
                // Attribute::ReadNone,
                Attribute::WillReturn,
            ],
            false,
        );
    }

    /// Saves the pointer to a stack variable, returning the pointer to the shadowed variable,
    /// if it exists.
    pub fn insert_stack_pointer(
        &mut self,
        name: String,
        pointer: Pointer<'ctx>,
    ) -> Option<Pointer<'ctx>> {
        self.stack.insert(name, pointer)
    }

    /// Gets the pointer to a stack variable.
    pub fn get_stack_pointer(&self, name: &str) -> Option<Pointer<'ctx>> {
        self.stack.get(name).copied()
    }

    /// Removes the pointer to a stack variable.
    pub fn remove_stack_pointer(&mut self, name: &str) {
        self.stack.remove(name);
    }

    /// Returns the return entity representation.
    pub fn r#return(&self) -> Return<'ctx> {
        self.r#return
    }

    /// Returns the pointer to the function return value.
    /// # Panics
    /// If the pointer has not been set yet.
    pub fn return_pointer(&self) -> Option<Pointer<'ctx>> {
        self.r#return.return_pointer()
    }

    /// Returns the return data size in bytes, based on the default stack alignment.
    /// # Panics
    /// If the pointer has not been set yet.
    pub fn return_data_size(&self) -> usize {
        self.r#return.return_data_size()
    }

    /// Returns the function entry block.
    pub fn entry_block(&self) -> inkwell::basic_block::BasicBlock<'ctx> {
        self.entry_block
    }

    /// Returns the function return block.
    pub fn return_block(&self) -> inkwell::basic_block::BasicBlock<'ctx> {
        self.return_block
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
            .expect("The EVM data must have been initialized")
    }

    /// Returns the EVM legacy assembly data mutable reference.
    /// # Panics
    /// If the EVM data has not been initialized.
    pub fn evmla_mut(&mut self) -> &mut EVMLAData<'ctx> {
        self.evmla_data
            .as_mut()
            .expect("The EVM data must have been initialized")
    }

    /// Sets the Vyper data.
    pub fn set_vyper_data(&mut self, data: VyperData) {
        self.vyper_data = Some(data);
    }

    /// Returns the Vyper data reference.
    /// # Panics
    /// If the Vyper data has not been initialized.
    pub fn vyper(&self) -> &VyperData {
        self.vyper_data
            .as_ref()
            .expect("The Vyper data must have been initialized")
    }

    /// Returns the Vyper data mutable reference.
    /// # Panics
    /// If the Vyper data has not been initialized.
    pub fn vyper_mut(&mut self) -> &mut VyperData {
        self.vyper_data
            .as_mut()
            .expect("The Vyper data must have been initialized")
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
}
