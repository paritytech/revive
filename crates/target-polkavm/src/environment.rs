use inkwell::{builder::Builder, context::Context, module::Module, values::FunctionValue};
use polkavm_common::elf::FnMetadata;

use revive_compilation_target::environment::Environment;
use revive_compilation_target::target::Target;

use crate::PolkaVm;

impl<'ctx> Environment<'ctx> for PolkaVm {
    fn call_start(&'ctx self, builder: &Builder<'ctx>, start: FunctionValue<'ctx>) -> Module<'ctx> {
        let module = self.context().create_module("entrypoint");

        let (call, deploy) = pvm_exports(&self.0);
        module.link_in_module(call).unwrap();
        module.link_in_module(deploy).unwrap();

        let function_type = self.context().void_type().fn_type(&[], false);

        let call = module.add_function("call", function_type, None);
        call.set_section(Some(".text.polkavm_export"));
        builder.position_at_end(self.context().append_basic_block(call, "entry"));
        builder.build_call(start, &[], "call_start");
        builder.build_return(None);

        let deploy = module.add_function("deploy", function_type, None);
        deploy.set_section(Some(".text.polkavm_export"));
        builder.position_at_end(self.context().append_basic_block(deploy, "entry"));
        builder.build_unreachable();
        builder.build_return(None);

        module
    }
}

pub(super) fn pvm_exports(context: &Context) -> (Module, Module) {
    let call_m = context.create_module("pvm_call");
    let deploy_m = context.create_module("pvm_deploy");

    call_m.set_inline_assembly(&generate_export_assembly("call"));
    deploy_m.set_inline_assembly(&generate_export_assembly("deploy"));

    (call_m, deploy_m)
}

fn generate_export_assembly(symbol: &str) -> String {
    let mut assembly = String::new();

    assembly.push_str(".pushsection .polkavm_exports,\"\",@progbits\n");
    assembly.push_str(".byte 1\n"); // Version.
    assembly.push_str(&format!(".4byte {symbol}\n")); // Address

    // Metadata
    let mut metadata = Vec::new();
    FnMetadata {
        name: symbol.to_string(),
        args: Default::default(),
        return_ty: Default::default(),
    }
    .serialize(|slice| metadata.extend_from_slice(slice));

    assembly.push_str(&bytes_to_asm(&metadata));

    assembly.push_str(".popsection\n");

    assembly
}

pub fn bytes_to_asm(bytes: &[u8]) -> String {
    use std::fmt::Write;

    let mut out = String::with_capacity(bytes.len() * 11);
    for &byte in bytes {
        writeln!(&mut out, ".byte 0x{:02x}", byte).unwrap();
    }

    out
}
