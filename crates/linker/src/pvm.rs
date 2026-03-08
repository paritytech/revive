//! The revive PVM blob linker library.

fn link_elf(code: &[u8], strip_binary: bool, optimize: bool) -> anyhow::Result<Vec<u8>> {
    let mut config = polkavm_linker::Config::default();
    config.set_strip(strip_binary);
    config.set_optimize(optimize);

    polkavm_linker::program_from_elf(config, polkavm_linker::TargetInstructionSet::ReviveV1, code)
        .map_err(|reason| anyhow::anyhow!("polkavm linker failed: {}", reason))
}

pub fn polkavm_linker<T: AsRef<[u8]>>(code: T, strip_binary: bool) -> anyhow::Result<Vec<u8>> {
    let code = code.as_ref();

    // The polkavm-linker has a known bug where its reachability validation
    // panics due to non-deterministic export ordering after optimization.
    // When this happens, retry without linker-level optimization.
    // The LLVM optimization pipeline already handles the heavy lifting.
    match std::panic::catch_unwind(|| link_elf(code, strip_binary, true)) {
        Ok(result) => result,
        Err(_) => link_elf(code, strip_binary, false),
    }
}
