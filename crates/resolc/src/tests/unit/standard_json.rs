use std::path::PathBuf;

use revive_solc_json_interface::SolcStandardJsonInput;

use crate::test_utils::build_yul_standard_json;

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
