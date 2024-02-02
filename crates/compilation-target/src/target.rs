use inkwell::{
    context::Context,
    module::Module,
    targets::{CodeModel, RelocMode},
    OptimizationLevel,
};

pub trait Target<'ctx> {
    const TARGET_NAME: &'ctx str;
    const TARGET_TRIPLE: &'ctx str;
    const TARGET_FEATURES: &'ctx str;
    const CPU: &'ctx str;
    const RELOC_MODE: RelocMode = RelocMode::Default;
    const CODE_MODEL: CodeModel = CodeModel::Default;

    fn initialize_llvm() {
        inkwell::targets::Target::initialize_riscv(&Default::default());
    }

    fn context(&self) -> &Context;

    fn libraries(&'ctx self) -> Vec<Module<'ctx>>;

    fn link(&self, blob: &[u8]) -> Vec<u8>;

    fn optimization_level(&self) -> OptimizationLevel;
}
