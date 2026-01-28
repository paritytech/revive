use std::path::PathBuf;

use revive_solc_json_interface::{
    SolcStandardJsonInput, SolcStandardJsonInputSettingsSelectionFileFlag,
};

use crate::{
    cli_utils::{STANDARD_JSON_PVM_CODEGEN_PER_FILE_PATH, STANDARD_JSON_YUL_PVM_CODEGEN_PATH},
    test_utils::build_yul_standard_json,
};

#[test]
fn standard_json_yul_solc() {
    let solc_input = SolcStandardJsonInput::try_from(Some(
        PathBuf::from("src/tests/data/standard_json/yul_solc.json").as_path(),
    ))
    .unwrap();
    let solc_output = build_yul_standard_json(solc_input).unwrap();

    assert!(!solc_output
        .contracts
        .get("Test")
        .expect("The `Test` contract is missing")
        .get("Return")
        .expect("The `Return` contract is missing")
        .evm
        .as_ref()
        .expect("The `evm` field is missing")
        .bytecode
        .as_ref()
        .expect("The `bytecode` field is missing")
        .object
        .is_empty())
}

#[test]
fn standard_json_yul_solc_urls() {
    let solc_input = SolcStandardJsonInput::try_from(Some(
        PathBuf::from("src/tests/data/standard_json/yul_solc_urls.json").as_path(),
    ))
    .unwrap();
    let solc_output = build_yul_standard_json(solc_input).unwrap();

    assert!(!solc_output
        .contracts
        .get("Test")
        .expect("The `Test` contract is missing")
        .get("Return")
        .expect("The `Return` contract is missing")
        .evm
        .as_ref()
        .expect("The `evm` field is missing")
        .bytecode
        .as_ref()
        .expect("The `bytecode` field is missing")
        .object
        .is_empty())
}

#[test]
fn standard_json_selection_to_prune_with_evm_child_per_file() {
    let solc_input = SolcStandardJsonInput::try_from(Some(
        PathBuf::from(STANDARD_JSON_PVM_CODEGEN_PER_FILE_PATH).as_path(),
    ))
    .unwrap();
    let selection_to_prune = solc_input.settings.selection_to_prune();

    // Verify that every flag exists in the selection to prune for the `all` wildcard.
    for flag in SolcStandardJsonInputSettingsSelectionFileFlag::all() {
        assert!(
            selection_to_prune.all.contains(*flag),
            "`{}` should be a selection to prune from the `all` wildcard settings",
            serde_json::to_string(flag).unwrap(),
        );
    }

    // The `abi`, `metadata`, `evm.methodIdentifiers`, `evm.bytecode` are requested
    // per file, thus all other flags except `evm` should be in the selection to prune.
    let expected_selection_to_prune_per_file = &[
        SolcStandardJsonInputSettingsSelectionFileFlag::Devdoc,
        SolcStandardJsonInputSettingsSelectionFileFlag::Userdoc,
        SolcStandardJsonInputSettingsSelectionFileFlag::StorageLayout,
        SolcStandardJsonInputSettingsSelectionFileFlag::AST,
        SolcStandardJsonInputSettingsSelectionFileFlag::Yul,
        SolcStandardJsonInputSettingsSelectionFileFlag::EVMLA,
        SolcStandardJsonInputSettingsSelectionFileFlag::EVMDBC,
        SolcStandardJsonInputSettingsSelectionFileFlag::Assembly,
        SolcStandardJsonInputSettingsSelectionFileFlag::Ir,
    ];

    for (file_name, selection_to_prune_per_file) in &selection_to_prune.files.files {
        // Verify that every expected flag exists in the selection to prune for a specific file.
        for flag in expected_selection_to_prune_per_file {
            assert!(
                selection_to_prune_per_file.contains(*flag),
                "`{}` should be a selection to prune from file `{file_name}`",
                serde_json::to_string(flag).unwrap()
            );
        }

        // Verify that every unexpected flag is omitted from the selection to prune.
        for flag in SolcStandardJsonInputSettingsSelectionFileFlag::all() {
            if !expected_selection_to_prune_per_file.contains(flag) {
                assert!(
                    !selection_to_prune_per_file.contains(*flag),
                    "`{}` should not be a selection to prune from file `{file_name}`",
                    serde_json::to_string(flag).unwrap()
                );
            }
        }
    }
}

#[test]
fn standard_json_selection_to_prune_with_evm_parent_for_all() {
    let solc_input = SolcStandardJsonInput::try_from(Some(
        PathBuf::from(STANDARD_JSON_YUL_PVM_CODEGEN_PATH).as_path(),
    ))
    .unwrap();
    let selection_to_prune = solc_input.settings.selection_to_prune();

    // Only the `evm` parent is requested in the `all` wildcard, thus all flags
    // except `evm` and its child flags should be in the selection to prune.
    // (Exception: `evm.legacyAssembly` is the only `evm` child flag that should also be pruned).
    let expected_selection_to_prune_for_all = &[
        SolcStandardJsonInputSettingsSelectionFileFlag::ABI,
        SolcStandardJsonInputSettingsSelectionFileFlag::Metadata,
        SolcStandardJsonInputSettingsSelectionFileFlag::Devdoc,
        SolcStandardJsonInputSettingsSelectionFileFlag::Userdoc,
        SolcStandardJsonInputSettingsSelectionFileFlag::StorageLayout,
        SolcStandardJsonInputSettingsSelectionFileFlag::AST,
        SolcStandardJsonInputSettingsSelectionFileFlag::Yul,
        SolcStandardJsonInputSettingsSelectionFileFlag::EVMLA,
        SolcStandardJsonInputSettingsSelectionFileFlag::Ir,
    ];

    // Verify that every expected flag exists in the selection to prune for the `all` wildcard.
    for flag in expected_selection_to_prune_for_all {
        assert!(
            selection_to_prune.all.contains(*flag),
            "`{}` should be a selection to prune from the `all` wildcard settings",
            serde_json::to_string(flag).unwrap()
        );
    }

    // Verify that every unexpected flag is omitted from the selection to prune for the `all` wildcard.
    for flag in SolcStandardJsonInputSettingsSelectionFileFlag::all() {
        if !expected_selection_to_prune_for_all.contains(flag) {
            assert!(
                !selection_to_prune.all.contains(*flag),
                "`{}` should not be a selection to prune from the `all` wildcard settings",
                serde_json::to_string(flag).unwrap(),
            );
        }
    }

    // Verify that there is nothing to be pruned for each file when there are no per-file requests.
    assert!(
        solc_input.settings.output_selection.files.is_empty(),
        "no output should be requested per file"
    );
    assert!(
        selection_to_prune.files.is_empty(),
        "no selections should be pruned per file when there are no per-file requests"
    );
}
