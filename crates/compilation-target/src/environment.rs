use inkwell::{builder::Builder, module::Module, values::FunctionValue};

/// [Environment] describes EVM runtime functionality.
pub trait Environment<'ctx> {
    const STACK_SIZE: u32 = 1024 * 32;
    const CALLDATA_SIZE: u32 = 0x10000;
    const RETURNDATA_SIZE: u32 = 0x10000;
    const MEMORY_SIZE: u32 = 0x100000;

    /// Build a module containing all required runtime exports and imports.
    ///
    /// The `start` function is the entrypoint to the contract logic.
    /// The returned `Module` is expected to call `start` somewhere.
    fn call_start(&'ctx self, builder: &Builder<'ctx>, start: FunctionValue<'ctx>) -> Module<'ctx>;
}
