use std::{
    fs::File,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

pub use resolc::{self, SolcCompiler};

/// The `resolc` CLI optimization settings used.
pub struct ResolcOptSettings;
impl ResolcOptSettings {
    pub const PERFORMANCE: &'static str = "-O3";
}

/// The `solc` CLI optimization settings used for `--optimize-runs`.
pub struct SolcOptSettings;
impl SolcOptSettings {
    pub const PERFORMANCE: &'static str = "20000";
}

/// Executes the `command` with the given `arguments` and `stdin_config`.
pub fn execute_command(command: &str, arguments: &[&str], stdin_config: Stdio) -> Output {
    Command::new(command)
        .args(arguments)
        .stdin(stdin_config)
        .stdout(Stdio::piped())
        .output()
        .unwrap()
}

/// Gets the configuration for the `stdin` handle when executing a command.
/// If a `stdin_file_path` is provided, it will be used in the configuration;
/// otherwise, the stream passed to `stdin` will be ignored.
pub fn get_stdin_config(stdin_file_path: Option<&str>) -> Stdio {
    match stdin_file_path {
        Some(path) => Stdio::from(File::open(path).unwrap()),
        None => Stdio::null(),
    }
}

/// Gets the absolute path of a file. The `relative_path` must
/// be relative to the `resolc` crate.
/// Panics if the path does not exist or is not an accessible file.
pub fn absolute_path(relative_path: &str) -> String {
    let absolute_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    if !absolute_path.is_file() {
        panic!("expected a file at `{}`", absolute_path.display());
    }

    absolute_path.to_string_lossy().into_owned()
}

// TODO: Remove allow dead code after moving helpers to `tests` module. (May move the tests as well.)
#[allow(dead_code)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_solidity_to_binary_blob() {
        let path = absolute_path("src/tests/data/solidity/contract.sol");
        let resolc_arguments = &[&path, "--bin", ResolcOptSettings::PERFORMANCE];
        let solc_arguments = &[
            &path,
            "--bin",
            "--via-ir",
            "--optimize",
            "--optimize-runs",
            SolcOptSettings::PERFORMANCE,
        ];

        let resolc_result = execute_command(
            resolc::DEFAULT_EXECUTABLE_NAME,
            resolc_arguments,
            get_stdin_config(None),
        );
        assert_pvm_blob(&resolc_result);

        let solc_result = execute_command(
            SolcCompiler::DEFAULT_EXECUTABLE_NAME,
            solc_arguments,
            get_stdin_config(None),
        );
        assert_evm_blob_from_solidity(&solc_result);
    }

    #[test]
    fn compiles_yul_to_binary_blob() {
        let path = absolute_path("src/tests/data/yul/memset.yul");
        let resolc_arguments = &[&path, "--yul", "--bin", ResolcOptSettings::PERFORMANCE];
        let solc_arguments = &[
            &path,
            "--strict-assembly",
            "--bin",
            "--optimize",
            "--optimize-runs",
            SolcOptSettings::PERFORMANCE,
        ];

        let resolc_result = execute_command(
            resolc::DEFAULT_EXECUTABLE_NAME,
            resolc_arguments,
            get_stdin_config(None),
        );
        assert_pvm_blob(&resolc_result);

        let solc_result = execute_command(
            SolcCompiler::DEFAULT_EXECUTABLE_NAME,
            solc_arguments,
            get_stdin_config(None),
        );
        assert_evm_blob_from_yul(&solc_result);
    }

    #[test]
    fn compiles_json_to_binary_blob() {
        let path = absolute_path("src/tests/data/standard_json/solidity_contracts.json");
        let resolc_arguments = &["--standard-json"];
        let solc_arguments = &["--standard-json"];

        let resolc_result = execute_command(
            resolc::DEFAULT_EXECUTABLE_NAME,
            resolc_arguments,
            get_stdin_config(Some(&path)),
        );
        assert_pvm_blob_from_json(&resolc_result, "src/Counter.sol", "Counter");

        let solc_result = execute_command(
            SolcCompiler::DEFAULT_EXECUTABLE_NAME,
            solc_arguments,
            get_stdin_config(Some(&path)),
        );
        assert_evm_blob_from_json(&solc_result, "src/Counter.sol", "Counter");
    }

    /// The starting hex value of a PVM blob (encoding of `"PVM"`)).
    const PVM_BLOB_START: &str = "50564d";

    /// The starting hex value of an EVM blob compiled from Solidity.
    const EVM_BLOB_START_FROM_SOLIDITY: &str = "6080";

    /// The starting hex value of an EVM blob compiled from Yul.
    /// (Blobs compiled from Yul do not have consistent starting hex values.)
    const EVM_BLOB_START_FROM_YUL: &str = "";

    /// Asserts that the `resolc` output contains a PVM blob.
    fn assert_pvm_blob(result: &Output) {
        assert_binary_blob(result, "Binary:\n", PVM_BLOB_START);
    }

    /// Asserts that the `solc` output from compiling Solidity contains an EVM blob.
    fn assert_evm_blob_from_solidity(result: &Output) {
        assert_binary_blob(result, "Binary:\n", EVM_BLOB_START_FROM_SOLIDITY);
    }

    /// Asserts that the `solc` output from compiling Yul contains an EVM blob.
    fn assert_evm_blob_from_yul(result: &Output) {
        assert_binary_blob(result, "Binary representation:\n", EVM_BLOB_START_FROM_YUL);
    }

    /// Asserts that the `resolc` output of compiling Solidity from JSON input contains a PVM blob.
    /// - `result`: The result of running the command.
    /// - `file_name`: The file name of the contract to verify the existence of a PVM blob for (corresponds to the name specified as a `source` in the JSON input).
    /// - `contract_name`: The name of the contract to verify the existence of a PVM blob for.
    fn assert_pvm_blob_from_json(result: &Output, file_name: &str, contract_name: &str) {
        assert_binary_blob_from_json(result, file_name, contract_name, PVM_BLOB_START);
    }

    /// Asserts that the `solc` output of compiling Solidity from JSON input contains an EVM blob.
    /// - `result`: The result of running the command.
    /// - `file_name`: The file name of the contract to verify the existence of an EVM blob for (corresponds to the name specified as a `source` in the JSON input).
    /// - `contract_name`: The name of the contract to verify the existence of an EVM blob for.
    fn assert_evm_blob_from_json(result: &Output, file_name: &str, contract_name: &str) {
        assert_binary_blob_from_json(
            result,
            file_name,
            contract_name,
            EVM_BLOB_START_FROM_SOLIDITY,
        );
    }

    /// Asserts that the output of a compilation contains a binary blob.
    /// - `result`: The result of running the command.
    /// - `blob_prefix`: The message immediately preceding the binary blob representation.
    /// - `blob_start`: The starting hex value of the binary blob.
    fn assert_binary_blob(result: &Output, blob_prefix: &str, blob_start: &str) {
        assert_command_success(result);

        let is_blob = to_string(&result.stdout)
            .split(blob_prefix)
            .collect::<Vec<&str>>()
            .get(1)
            .is_some_and(|blob| blob.starts_with(blob_start));

        assert!(
            is_blob,
            "expected a binary blob starting with `{blob_start}`",
        );
    }

    /// Asserts that the output of compiling Solidity from JSON input contains a binary blob.
    /// - `result`: The result of running the command.
    /// - `file_name`: The file name of the contract to verify the existence of a binary blob for (corresponds to the name specified as a `source` in the JSON input).
    /// - `contract_name`: The name of the contract to verify the existence of a binary blob for.
    /// - `blob_start`: The starting hex value of the binary blob.
    ///
    /// See [output description](https://docs.soliditylang.org/en/latest/using-the-compiler.html#output-description)
    /// for more details on the JSON output format.
    fn assert_binary_blob_from_json(
        result: &Output,
        file_name: &str,
        contract_name: &str,
        blob_start: &str,
    ) {
        assert_command_success(result);

        let output = to_string(&result.stdout);
        let parsed_output: serde_json::Value =
            serde_json::from_str(&output).expect("expected valid JSON output");
        let contract = &parsed_output["contracts"][file_name][contract_name];

        let errors = contract["errors"].as_array();
        assert!(
            errors.is_none(),
            "errors found for JSON-provided contract `{contract_name}` in `{file_name}`: {}",
            get_first_json_error(errors.unwrap()),
        );

        let blob = contract["evm"]["bytecode"]["object"]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "expected a binary blob for JSON-provided contract `{contract_name}` in `{file_name}`",
                )
            });
        assert!(
            blob.starts_with(blob_start),
            "expected a binary blob starting with `{blob_start}`",
        );
    }

    /// Asserts that the command terminated successfully with a `0` exit code.
    fn assert_command_success(result: &Output) {
        assert!(
            result.status.success(),
            "command failed: {}",
            to_string(&result.stderr)
        );
    }

    /// Gets the first error message reported when compiling from JSON input.
    ///
    /// See [output description](https://docs.soliditylang.org/en/latest/using-the-compiler.html#output-description)
    /// for more details on the JSON output format.
    fn get_first_json_error(errors: &[serde_json::Value]) -> &str {
        errors.first().unwrap()["message"].as_str().unwrap()
    }

    /// Converts bytes to a string.
    fn to_string(result: &[u8]) -> String {
        String::from_utf8_lossy(result).to_string()
    }
}
