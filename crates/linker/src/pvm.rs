//! The revive PVM blob linker library.

/// Links the ELF `code` into a PVM blob.
///
/// A `metadata_hash` is emitted into the blob's metadata hash section.
pub fn polkavm_linker<T: AsRef<[u8]>>(
    code: T,
    strip_binary: bool,
    metadata_hash: Option<Vec<u8>>,
) -> anyhow::Result<Vec<u8>> {
    let mut config = polkavm_linker::Config::default();
    config.set_strip(strip_binary);
    config.set_optimize(true);
    config.set_metadata_hash(metadata_hash);

    polkavm_linker::program_from_elf(
        config,
        polkavm_linker::TargetInstructionSet::ReviveV1,
        code.as_ref(),
    )
    .map_err(|reason| anyhow::anyhow!("polkavm linker failed: {}", reason))
}
