//! The revive PVM blob linker library.

pub fn polkavm_linker<T: AsRef<[u8]>>(code: T, strip_binary: bool) -> anyhow::Result<Vec<u8>> {
    let mut config = polkavm_linker::Config::default();
    config.set_strip(strip_binary);
    config.set_optimize(true);

    polkavm_linker::program_from_elf(
        config,
        polkavm_linker::TargetInstructionSet::ReviveV1,
        code.as_ref(),
    )
    .map_err(|reason| anyhow::anyhow!("polkavm linker failed: {}", reason))
}
