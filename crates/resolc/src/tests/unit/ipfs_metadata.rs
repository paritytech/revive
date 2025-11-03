use std::path::PathBuf;

use revive_common::MetadataHash;
use revive_llvm_context::{initialize_llvm, DebugConfig, OptimizerSettings, PolkaVMTarget};
use revive_solc_json_interface::SolcStandardJsonInputSettingsLibraries;

use crate::process::native_process::EXECUTABLE;
use crate::project::Project;
use crate::DEFAULT_EXECUTABLE_NAME;

#[test]
fn compiles_with_ipfs_metadata_hash_and_emits_multihash() {
    let debug = DebugConfig::new(None, true);

    let resolc_path = std::env::var("CARGO_BIN_EXE_resolc")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_EXECUTABLE_NAME));
    let _ = EXECUTABLE.set(resolc_path);

    initialize_llvm(PolkaVMTarget::PVM, DEFAULT_EXECUTABLE_NAME, &[]);
    let project = Project::try_from_yul_paths(
        &[PathBuf::from("src/tests/data/yul/Test.yul")],
        None,
        SolcStandardJsonInputSettingsLibraries::default(),
        &debug,
    )
    .expect("project from yul");

    let mut messages = Vec::new();
    let build = project
        .compile(
            &mut messages,
            OptimizerSettings::none(),
            MetadataHash::IPFS,
            &debug,
            &[],
            Default::default(),
        )
        .expect("compile should succeed");

    assert!(
        messages.is_empty(),
        "No errors expected, got: {:?}",
        messages
    );

    let (.., result) = build
        .results
        .into_iter()
        .next()
        .expect("one contract result");
    let contract = result.expect("contract built successfully");

    let bytes = contract
        .build
        .metadata_hash
        .as_ref()
        .expect("metadata hash should be present");
    let slice: &[u8] = &bytes[..];

    assert_eq!(slice.len(), 34, "multihash length must be 34 bytes");
    assert_eq!(
        &slice[0..2],
        &[0x12, 0x20],
        "multihash prefix must be sha2-256 (0x12 0x20)"
    );
}
