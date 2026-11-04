use assert_fs::{prelude::*, TempDir};
use revive_llvm_builder::LLVMPath;

pub const REVIVE_LLVM: &str = "revive-llvm";

/// The repository root relative to this crate's manifest directory.
const REPOSITORY_ROOT: &str = "../..";

/// Matches the LLVM patch files the builder applies.
const PATCH_FILE_PATTERN: &str = "*.patch";

pub struct TestDir {
    _tempdir: TempDir,
    path: std::path::PathBuf,
}

/// Creates a temporary directory for testing with submodule and LLVM patches setup.
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
                "release/23.x",
                "https://github.com/llvm/llvm-project.git",
                "llvm",
            ])
            .current_dir(tmppath)
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
            .current_dir(tmppath)
            .output()?;

        let patches = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(REPOSITORY_ROOT)
            .join(LLVMPath::DIRECTORY_PATCHES);
        if patches.exists() {
            tempdir
                .child(LLVMPath::DIRECTORY_PATCHES)
                .copy_from(patches, &[PATCH_FILE_PATTERN])?;
        }

        Ok(Self {
            path: tmppath.to_path_buf(),
            _tempdir: tempdir,
        })
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}
