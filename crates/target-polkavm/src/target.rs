use inkwell::{
    context::Context, memory_buffer::MemoryBuffer, module::Module, targets::RelocMode,
    OptimizationLevel,
};

pub const TARGET_NAME: &str = "riscv32";
pub const TARGET_TRIPLE: &str = "riscv32-unknown-unknown-elf";
pub const TARGET_FEATURES: &str = "+e,+m";
pub const CPU: &str = "generic-rv32";
pub const RELOC_MODE: RelocMode = RelocMode::PIC;

pub fn libraries(context: &Context) -> Vec<Module<'_>> {
    let guest_bitcode = include_bytes!("../polkavm_guest.bc");
    let imports = MemoryBuffer::create_from_memory_range(guest_bitcode, "guest_bc");

    vec![Module::parse_bitcode_from_buffer(&imports, context).unwrap()]
}

pub fn link(blob: &[u8]) -> Vec<u8> {
    crate::linker::link(blob)
}

pub fn optimization_level() -> OptimizationLevel {
    OptimizationLevel::Aggressive
}
