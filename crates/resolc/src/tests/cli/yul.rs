//! The tests for running resolc with yul option.

use crate::cli_utils::{
    assert_command_failure, assert_command_success, assert_equal_exit_codes, execute_resolc,
    execute_solc, RESOLC_YUL_FLAG, SOLC_YUL_FLAG, YUL_CONTRACT_PATH, YUL_DEPLOY_ONLY_OBJECT_PATH,
    YUL_DUPLICATE_FUNCTIONS_DEEP_NESTING_PATH, YUL_DUPLICATE_FUNCTIONS_SWITCH_PATH,
    YUL_FACTORY_DEPENDENCY_NESTED_OBJECT_PATH, YUL_INVALID_HEX_NIBBLES_PATH,
};

#[test]
fn runs_with_valid_input_file() {
    let resolc_result = execute_resolc(&[YUL_CONTRACT_PATH, RESOLC_YUL_FLAG]);
    assert_command_success(&resolc_result, "Providing a valid input file");

    assert!(resolc_result
        .stderr
        .contains("Compiler run successful. No output requested"));

    let solc_result = execute_solc(&[YUL_CONTRACT_PATH, SOLC_YUL_FLAG]);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn fails_without_input_file() {
    let resolc_result = execute_resolc(&[RESOLC_YUL_FLAG]);
    assert_command_failure(&resolc_result, "Omitting an input file");

    let solc_result = execute_solc(&[SOLC_YUL_FLAG]);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn bails_with_invalid_input_file() {
    let resolc_result = execute_resolc(&[YUL_INVALID_HEX_NIBBLES_PATH, RESOLC_YUL_FLAG]);
    assert_command_failure(&resolc_result, "Providing an invalid input file");

    let solc_result = execute_solc(&[YUL_INVALID_HEX_NIBBLES_PATH, SOLC_YUL_FLAG]);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

#[test]
fn duplicate_functions_in_switch_cases() {
    let resolc_result = execute_resolc(&[
        YUL_DUPLICATE_FUNCTIONS_SWITCH_PATH,
        RESOLC_YUL_FLAG,
        "--bin",
    ]);
    assert_command_success(
        &resolc_result,
        "Duplicate function names in different switch cases",
    );
}

#[test]
fn duplicate_functions_deep_nesting() {
    let resolc_result = execute_resolc(&[
        YUL_DUPLICATE_FUNCTIONS_DEEP_NESTING_PATH,
        RESOLC_YUL_FLAG,
        "--bin",
    ]);
    assert_command_success(
        &resolc_result,
        "Duplicate function names in deeply nested switch cases",
    );
}

#[test]
fn compiles_object_without_a_runtime_sub_object() {
    let resolc_result = execute_resolc(&[YUL_DEPLOY_ONLY_OBJECT_PATH, RESOLC_YUL_FLAG, "--bin"]);
    assert_command_success(
        &resolc_result,
        "An object carrying deploy code only, without a `_deployed` sub-object",
    );

    let solc_result = execute_solc(&[YUL_DEPLOY_ONLY_OBJECT_PATH, SOLC_YUL_FLAG]);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}

/// A factory dependency given only as a nested object is compiled too.
#[test]
fn compiles_factory_dependency_nested_object() {
    let resolc_result = execute_resolc(&[
        YUL_FACTORY_DEPENDENCY_NESTED_OBJECT_PATH,
        RESOLC_YUL_FLAG,
        "--bin",
    ]);
    assert_command_success(&resolc_result, "A factory dependency as a nested object");
    for object in ["K_4", "P_19"] {
        assert!(
            resolc_result.stdout.contains(&format!(
                "{YUL_FACTORY_DEPENDENCY_NESTED_OBJECT_PATH}:{object}"
            )),
            "the object `{object}` should be compiled"
        );
    }

    let solc_result = execute_solc(&[YUL_FACTORY_DEPENDENCY_NESTED_OBJECT_PATH, SOLC_YUL_FLAG]);
    assert_equal_exit_codes(&solc_result, &resolc_result);
}
