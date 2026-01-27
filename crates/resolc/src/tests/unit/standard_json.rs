use std::path::PathBuf;

use revive_solc_json_interface::{
    SolcStandardJsonInput, SolcStandardJsonInputSettingsSelectionFileFlag,
};

use crate::{
    cli_utils::STANDARD_JSON_PVM_CODEGEN_PER_FILE_PATH, test_utils::build_yul_standard_json,
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
fn standard_json_prune_output_selection() {
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
