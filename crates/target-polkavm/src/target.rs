use inkwell::{
    context::Context, memory_buffer::MemoryBuffer, module::Module, targets::RelocMode,
    OptimizationLevel,
};
use revive_compilation_target::target::Target;

use crate::PolkaVm;

impl<'ctx> Target<'ctx> for PolkaVm {
    const TARGET_NAME: &'static str = "riscv32";
    const TARGET_TRIPLE: &'static str = "riscv32-unknown-unknown-elf";
    const TARGET_FEATURES: &'static str = "+e,+m";
    const CPU: &'static str = "generic-rv32";
    const RELOC_MODE: RelocMode = RelocMode::PIC;

    fn libraries(&'ctx self) -> Vec<Module<'ctx>> {
        let guest_bitcode = include_bytes!("../polkavm_guest.bc");
        let imports = MemoryBuffer::create_from_memory_range(guest_bitcode, "guest_bc");

        vec![Module::parse_bitcode_from_buffer(&imports, &self.0).unwrap()]
    }

    fn context(&self) -> &Context {
        &self.0
    }

    fn link(&self, blob: &[u8]) -> Vec<u8> {
        crate::linker::link(blob)
    }

    fn optimization_level(&self) -> OptimizationLevel {
        OptimizationLevel::Aggressive
    }
}
