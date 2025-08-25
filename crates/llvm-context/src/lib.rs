//! The LLVM context library.

use std::ffi::CString;
use std::sync::OnceLock;

pub use self::debug_config::ir_type::IRType as DebugConfigIR;
pub use self::debug_config::DebugConfig;
pub use self::optimizer::settings::size_level::SizeLevel as OptimizerSettingsSizeLevel;
pub use self::optimizer::settings::Settings as OptimizerSettings;
pub use self::optimizer::Optimizer;
pub use self::polkavm::build as polkavm_build;
pub use self::polkavm::context::address_space::AddressSpace as PolkaVMAddressSpace;
pub use self::polkavm::context::argument::Argument as PolkaVMArgument;
pub use self::polkavm::context::attribute::Attribute as PolkaVMAttribute;
pub use self::polkavm::context::build::Build as PolkaVMBuild;
pub use self::polkavm::context::code_type::CodeType as PolkaVMCodeType;
pub use self::polkavm::context::debug_info::DebugInfo;
pub use self::polkavm::context::function::declaration::Declaration as PolkaVMFunctionDeclaration;
pub use self::polkavm::context::function::intrinsics::Intrinsics as PolkaVMIntrinsicFunction;
pub use self::polkavm::context::function::llvm_runtime::LLVMRuntime as PolkaVMLLVMRuntime;
pub use self::polkavm::context::function::r#return::Return as PolkaVMFunctionReturn;
pub use self::polkavm::context::function::runtime::arithmetics::Division as PolkaVMDivisionFunction;
pub use self::polkavm::context::function::runtime::arithmetics::Remainder as PolkaVMRemainderFunction;
pub use self::polkavm::context::function::runtime::arithmetics::SignedDivision as PolkaVMSignedDivisionFunction;
pub use self::polkavm::context::function::runtime::arithmetics::SignedRemainder as PolkaVMSignedRemainderFunction;
pub use self::polkavm::context::function::runtime::deploy_code::DeployCode as PolkaVMDeployCodeFunction;
pub use self::polkavm::context::function::runtime::entry::Entry as PolkaVMEntryFunction;
pub use self::polkavm::context::function::runtime::revive::Exit as PolkaVMExitFunction;
pub use self::polkavm::context::function::runtime::revive::WordToPointer as PolkaVMWordToPointerFunction;
pub use self::polkavm::context::function::runtime::runtime_code::RuntimeCode as PolkaVMRuntimeCodeFunction;
pub use self::polkavm::context::function::runtime::sbrk::Sbrk as PolkaVMSbrkFunction;
pub use self::polkavm::context::function::runtime::FUNCTION_DEPLOY_CODE as PolkaVMFunctionDeployCode;
pub use self::polkavm::context::function::runtime::FUNCTION_ENTRY as PolkaVMFunctionEntry;
pub use self::polkavm::context::function::runtime::FUNCTION_RUNTIME_CODE as PolkaVMFunctionRuntimeCode;
pub use self::polkavm::context::function::yul_data::YulData as PolkaVMFunctionYulData;
pub use self::polkavm::context::function::Function as PolkaVMFunction;
pub use self::polkavm::context::global::Global as PolkaVMGlobal;
pub use self::polkavm::context::pointer::heap::LoadWord as PolkaVMLoadHeapWordFunction;
pub use self::polkavm::context::pointer::heap::StoreWord as PolkaVMStoreHeapWordFunction;
pub use self::polkavm::context::pointer::storage::LoadTransientWord as PolkaVMLoadTransientStorageWordFunction;
pub use self::polkavm::context::pointer::storage::LoadWord as PolkaVMLoadStorageWordFunction;
pub use self::polkavm::context::pointer::storage::StoreTransientWord as PolkaVMStoreTransientStorageWordFunction;
pub use self::polkavm::context::pointer::storage::StoreWord as PolkaVMStoreStorageWordFunction;
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
pub use self::polkavm::evm::event::EventLog as PolkaVMEventLogFunction;
pub use self::polkavm::evm::ext_code as polkavm_evm_ext_code;
pub use self::polkavm::evm::immutable as polkavm_evm_immutable;
pub use self::polkavm::evm::immutable::Load as PolkaVMLoadImmutableDataFunction;
pub use self::polkavm::evm::immutable::Store as PolkaVMStoreImmutableDataFunction;
pub use self::polkavm::evm::math as polkavm_evm_math;
pub use self::polkavm::evm::memory as polkavm_evm_memory;
pub use self::polkavm::evm::r#return as polkavm_evm_return;
pub use self::polkavm::evm::return_data as polkavm_evm_return_data;
pub use self::polkavm::evm::storage as polkavm_evm_storage;
pub use self::polkavm::r#const as polkavm_const;
pub use self::polkavm::DummyLLVMWritable as PolkaVMDummyLLVMWritable;
pub use self::polkavm::WriteLLVM as PolkaVMWriteLLVM;
pub use self::target_machine::target::Target;
pub use self::target_machine::TargetMachine;

pub(crate) mod debug_config;
pub(crate) mod optimizer;
pub(crate) mod polkavm;
pub(crate) mod target_machine;

static DID_INITIALIZE: OnceLock<()> = OnceLock::new();

/// Initializes the LLVM compiler backend.
///
/// This is a no-op if called subsequentially.
///
/// `llvm_arguments` are passed as-is to the LLVM CL options parser.
pub fn initialize_llvm(target: Target, name: &str, llvm_arguments: &[String]) {
    let Ok(_) = DID_INITIALIZE.set(()) else {
        return; // Tests don't go through a recursive process
    };

    let argv = [name.to_string()]
        .iter()
        .chain(llvm_arguments)
        .map(|arg| CString::new(arg.as_bytes()).unwrap())
        .collect::<Vec<_>>();
    let argv: Vec<*const libc::c_char> = argv.iter().map(|arg| arg.as_ptr()).collect();
    let overview = CString::new("").unwrap();
    unsafe {
        inkwell::llvm_sys::support::LLVMParseCommandLineOptions(
            argv.len() as i32,
            argv.as_ptr(),
            overview.as_ptr(),
        );
    }

    inkwell::support::enable_llvm_pretty_stack_trace();

    match target {
        Target::PVM => inkwell::targets::Target::initialize_riscv(&Default::default()),
    }
}
