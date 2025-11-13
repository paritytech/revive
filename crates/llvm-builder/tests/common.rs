use assert_fs::TempDir;

pub const REVIVE_LLVM: &str = "revive-llvm";

pub struct TestDir {
    _tempdir: TempDir,
    path: std::path::PathBuf,
}

/// Creates a temporary directory for testing with submodule setup.
impl TestDir {
    pub fn new() -> anyhow::Result<Self> {
        let tempdir = TempDir::new()?;
        let tmppath = tempdir.path();

        // Initialize a git repo and add the LLVM submodule
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(tmppath)
            .output()?;

        std::process::Command::new("git")
            .args([
                "submodule",
                "add",
                "-b",
                "release/18.x",
                "https://github.com/llvm/llvm-project.git",
                "llvm",
            ])
            .current_dir(&tmppath)
            .output()?;

        std::process::Command::new("git")
            .args([
                "submodule",
                "update",
                "--init",
                "--recursive",
                "--force",
                "--depth 1",
            ])
            .current_dir(&tmppath)
            .output()?;

        Ok(Self {
            path: tmppath.to_path_buf(),
            _tempdir: tempdir,
        })
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}
