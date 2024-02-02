use inkwell::{
    module::Module,
    support::LLVMString,
    targets::{RelocMode, TargetTriple},
};
use revive_compilation_target::target::Target;

pub(crate) fn create<'ctx, T>(target: &'ctx T) -> Result<Module<'ctx>, LLVMString>
where
    T: Target<'ctx>,
{
    let module = target.context().create_module("contract");

    module.set_triple(&TargetTriple::create(<T as Target>::TARGET_TRIPLE));
    module.set_source_file_name("contract.bin");

    set_flags(target, &module);

    for lib in target.libraries() {
        module.link_in_module(lib)?;
    }

    Ok(module)
}

fn set_flags<'ctx, T>(target: &'ctx T, module: &Module<'ctx>)
where
    T: Target<'ctx>,
{
    if let RelocMode::PIC = <T as Target>::RELOC_MODE {
        module.add_basic_value_flag(
            "PIE Level",
            inkwell::module::FlagBehavior::Override,
            target.context().i32_type().const_int(2, false),
        );
    }
}
