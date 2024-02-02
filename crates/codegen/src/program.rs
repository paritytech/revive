use inkwell::{
    builder::Builder,
    module::{Linkage, Module},
    support::LLVMString,
    targets::{FileType, TargetTriple},
    values::{FunctionValue, GlobalValue},
    AddressSpace,
};

use revive_compilation_target::environment::Environment;
use revive_compilation_target::target::Target;

use crate::module;

pub struct Program<'ctx, T> {
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    pub calldata: GlobalValue<'ctx>,
    pub returndata: GlobalValue<'ctx>,
    pub target: &'ctx T,
    pub start: FunctionValue<'ctx>,
}

impl<'ctx, T> Program<'ctx, T>
where
    T: Target<'ctx> + Environment<'ctx>,
{
    pub fn new(target: &'ctx T) -> Result<Self, LLVMString> {
        T::initialize_llvm();

        let context = target.context();

        let module = module::create(target)?;
        let builder = context.create_builder();
        let address_space = Some(AddressSpace::default());

        let calldata_type = context.i8_type().array_type(T::CALLDATA_SIZE);
        let calldata = module.add_global(calldata_type, address_space, "calldata");

        let returndata_type = context.i8_type().array_type(T::RETURNDATA_SIZE);
        let returndata = module.add_global(returndata_type, address_space, "returndata");

        let start_fn_type = target.context().void_type().fn_type(&[], false);
        let start = module.add_function("start", start_fn_type, Some(Linkage::Internal));

        Ok(Self {
            module,
            builder,
            calldata,
            returndata,
            target,
            start,
        })
    }

    pub fn emit(&self, program: revive_ir::cfg::Program) {
        self.emit_start();
    }

    pub fn compile_and_link(&self) -> Vec<u8> {
        inkwell::targets::Target::from_name(T::TARGET_NAME)
            .expect("target name should be valid")
            .create_target_machine(
                &TargetTriple::create(T::TARGET_TRIPLE),
                T::CPU,
                T::TARGET_FEATURES,
                self.target.optimization_level(),
                T::RELOC_MODE,
                T::CODE_MODEL,
            )
            .expect("target configuration should be valid")
            .write_to_memory_buffer(&self.module, FileType::Object)
            .map(|out| self.target.link(out.as_slice()))
            .expect("linker should succeed")
            .to_vec()
    }

    fn emit_start(&self) {
        let start = self.start;
        let block = self
            .start
            .get_last_basic_block()
            .unwrap_or_else(|| self.target.context().append_basic_block(start, "entry"));

        self.builder.position_at_end(block);
        self.builder.build_return(None);

        let env_start = self.target.call_start(&self.builder, self.start);
        self.module
            .link_in_module(env_start)
            .expect("entrypoint module should be linkable");
    }
}
