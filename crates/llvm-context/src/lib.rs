//! The LLVM context library.

pub(crate) mod debug_config;
pub(crate) mod optimizer;
pub(crate) mod polkavm;
pub(crate) mod target_machine;

pub use self::debug_config::ir_type::IRType as DebugConfigIR;
pub use self::debug_config::DebugConfig;
pub use self::optimizer::settings::size_level::SizeLevel as OptimizerSettingsSizeLevel;
pub use self::optimizer::settings::Settings as OptimizerSettings;
pub use self::optimizer::Optimizer;
pub use self::polkavm::build_assembly_text as polkavm_build_assembly_text;
pub use self::polkavm::context::address_space::AddressSpace as PolkaVMAddressSpace;
pub use self::polkavm::context::argument::Argument as PolkaVMArgument;
pub use self::polkavm::context::attribute::Attribute as PolkaVMAttribute;
pub use self::polkavm::context::build::Build as PolkaVMBuild;
pub use self::polkavm::context::code_type::CodeType as PolkaVMCodeType;
pub use self::polkavm::context::debug_info::DebugInfo;
pub use self::polkavm::context::evmla_data::EVMLAData as PolkaVMContextEVMLAData;
pub use self::polkavm::context::function::block::evmla_data::key::Key as PolkaVMFunctionBlockKey;
pub use self::polkavm::context::function::block::evmla_data::EVMLAData as PolkaVMFunctionBlockEVMLAData;
pub use self::polkavm::context::function::block::Block as PolkaVMFunctionBlock;
pub use self::polkavm::context::function::declaration::Declaration as PolkaVMFunctionDeclaration;
pub use self::polkavm::context::function::evmla_data::EVMLAData as PolkaVMFunctionEVMLAData;
pub use self::polkavm::context::function::intrinsics::Intrinsics as PolkaVMIntrinsicFunction;
pub use self::polkavm::context::function::llvm_runtime::LLVMRuntime as PolkaVMLLVMRuntime;
pub use self::polkavm::context::function::r#return::Return as PolkaVMFunctionReturn;
pub use self::polkavm::context::function::runtime::deploy_code::DeployCode as PolkaVMDeployCodeFunction;
pub use self::polkavm::context::function::runtime::entry::Entry as PolkaVMEntryFunction;
pub use self::polkavm::context::function::runtime::immutable_data_load::ImmutableDataLoad as PolkaVMImmutableDataLoadFunction;
pub use self::polkavm::context::function::runtime::runtime_code::RuntimeCode as PolkaVMRuntimeCodeFunction;
pub use self::polkavm::context::function::runtime::FUNCTION_DEPLOY_CODE as PolkaVMFunctionDeployCode;
pub use self::polkavm::context::function::runtime::FUNCTION_ENTRY as PolkaVMFunctionEntry;
pub use self::polkavm::context::function::runtime::FUNCTION_LOAD_IMMUTABLE_DATA as PolkaVMFunctionImmutableDataLoad;
pub use self::polkavm::context::function::runtime::FUNCTION_RUNTIME_CODE as PolkaVMFunctionRuntimeCode;
pub use self::polkavm::context::function::yul_data::YulData as PolkaVMFunctionYulData;
pub use self::polkavm::context::function::Function as PolkaVMFunction;
pub use self::polkavm::context::global::Global as PolkaVMGlobal;
pub use self::polkavm::context::pointer::Pointer as PolkaVMPointer;
pub use self::polkavm::context::r#loop::Loop as PolkaVMLoop;
pub use self::polkavm::context::solidity_data::SolidityData as PolkaVMContextSolidityData;
pub use self::polkavm::context::yul_data::YulData as PolkaVMContextYulData;
pub use self::polkavm::context::Context as PolkaVMContext;
pub use self::polkavm::evm::arithmetic as polkavm_evm_arithmetic;
pub use self::polkavm::evm::bitwise as polkavm_evm_bitwise;
pub use self::polkavm::evm::call as polkavm_evm_call;
pub use self::polkavm::evm::calldata as polkavm_evm_calldata;
pub use self::polkavm::evm::comparison as polkavm_evm_comparison;
pub use self::polkavm::evm::context as polkavm_evm_contract_context;
pub use self::polkavm::evm::create as polkavm_evm_create;
pub use self::polkavm::evm::crypto as polkavm_evm_crypto;
pub use self::polkavm::evm::ether_gas as polkavm_evm_ether_gas;
pub use self::polkavm::evm::event as polkavm_evm_event;
pub use self::polkavm::evm::ext_code as polkavm_evm_ext_code;
pub use self::polkavm::evm::immutable as polkavm_evm_immutable;
pub use self::polkavm::evm::math as polkavm_evm_math;
pub use self::polkavm::evm::memory as polkavm_evm_memory;
pub use self::polkavm::evm::r#return as polkavm_evm_return;
pub use self::polkavm::evm::return_data as polkavm_evm_return_data;
pub use self::polkavm::evm::storage as polkavm_evm_storage;
pub use self::polkavm::metadata_hash::MetadataHash as PolkaVMMetadataHash;
pub use self::polkavm::r#const as polkavm_const;
pub use self::polkavm::utils as polkavm_utils;
pub use self::polkavm::Dependency as PolkaVMDependency;
pub use self::polkavm::DummyDependency as PolkaVMDummyDependency;
pub use self::polkavm::DummyLLVMWritable as PolkaVMDummyLLVMWritable;
pub use self::polkavm::WriteLLVM as PolkaVMWriteLLVM;
pub use self::target_machine::target::Target;
pub use self::target_machine::TargetMachine;

/// Initializes the target machine.
pub fn initialize_target(target: Target) {
    match target {
        Target::PVM => self::polkavm::initialize_target(),
    }
}
