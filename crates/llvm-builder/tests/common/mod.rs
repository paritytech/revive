use assert_fs::fixture::FileWriteStr;

pub const REVIVE_LLVM: &str = "revive-llvm";
pub const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REVIVE_LLVM_REPO_URL: &str = "https://github.com/llvm/llvm-project";
pub const REVIVE_LLVM_REPO_TEST_BRANCH: &str = "release/18.x";
pub const REVIVE_LLVM_REPO_TEST_SHA_INVALID: &str = "12345abcd";
pub const LLVM_LOCK_FILE: &str = "LLVM.lock";

/// Creates a temporary lock file for testing.
pub fn create_test_tmp_lockfile(
    reference: Option<String>,
) -> anyhow::Result<assert_fs::NamedTempFile> {
    let file = assert_fs::NamedTempFile::new(LLVM_LOCK_FILE)?;
    let lock = revive_llvm_builder::Lock {
        url: REVIVE_LLVM_REPO_URL.to_string(),
        branch: REVIVE_LLVM_REPO_TEST_BRANCH.to_string(),
        r#ref: reference,
    };
    file.write_str(toml::to_string(&lock)?.as_str())?;
    Ok(file)
}
