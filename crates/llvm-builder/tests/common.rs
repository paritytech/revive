use assert_fs::fixture::FileWriteStr;

pub const REVIVE_LLVM: &str = "revive-llvm";
pub const REVIVE_LLVM_REPO_URL: &str = "https://github.com/llvm/llvm-project";
pub const REVIVE_LLVM_REPO_TEST_BRANCH: &str = "release/18.x";

pub struct TestDir {
    _lockfile: assert_fs::NamedTempFile,
    path: std::path::PathBuf,
}

/// Creates a temporary lock file for testing.
impl TestDir {
    pub fn with_lockfile(reference: Option<String>) -> anyhow::Result<Self> {
        let file =
            assert_fs::NamedTempFile::new(revive_llvm_builder::lock::LLVM_LOCK_DEFAULT_PATH)?;
        let lock = revive_llvm_builder::Lock {
            url: REVIVE_LLVM_REPO_URL.to_string(),
            branch: REVIVE_LLVM_REPO_TEST_BRANCH.to_string(),
            r#ref: reference,
        };
        file.write_str(toml::to_string(&lock)?.as_str())?;

        Ok(Self {
            path: file
                .parent()
                .expect("lockfile parent dir always exists")
                .into(),
            _lockfile: file,
        })
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}
