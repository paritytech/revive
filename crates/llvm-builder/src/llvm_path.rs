//! The revive LLVM builder constants.

use std::path::PathBuf;
use std::sync::OnceLock;

pub static DIRECTORY_LLVM_TARGET: OnceLock<PathBuf> = OnceLock::new();

/// The LLVM path resolver.
pub struct LLVMPath {}

impl LLVMPath {
    /// The LLVM source directory.
    pub const DIRECTORY_LLVM_SOURCE: &'static str = "./llvm/";

    /// The LLVM host source directory for stage 1 of multistage MUSL and Emscripten builds.
    ///
    /// We use upstream LLVM anyways; re-use the same tree for host and target builds.
    pub const DIRECTORY_LLVM_HOST_SOURCE: &'static str = Self::DIRECTORY_LLVM_SOURCE;

    /// The Emscripten SDK source directory.
    pub const DIRECTORY_EMSDK_SOURCE: &'static str = "./emsdk/";

    /// Returns the path to the `llvm` stage 1 host LLVM source module directory.
    pub fn llvm_host_module_llvm() -> anyhow::Result<PathBuf> {
        let mut path = PathBuf::from(Self::DIRECTORY_LLVM_HOST_SOURCE);
        path.push("llvm");
        crate::utils::absolute_path(path).inspect(|absolute_path| {
            log::debug!(
                "llvm stage 1 host llvm source module: {}",
                absolute_path.display()
            )
        })
    }

    /// Returns the path to the `llvm` LLVM source module directory.
    pub fn llvm_module_llvm() -> anyhow::Result<PathBuf> {
        let mut path = PathBuf::from(Self::DIRECTORY_LLVM_SOURCE);
        path.push("llvm");
        crate::utils::absolute_path(path)
            .inspect(|absolute_path| log::debug!("llvm source module: {}", absolute_path.display()))
    }

    /// Returns the path to the MUSL source.
    pub fn musl_source(name: &str) -> anyhow::Result<PathBuf> {
        let mut path = PathBuf::from(DIRECTORY_LLVM_TARGET.get().unwrap());
        path.push(name);
        crate::utils::absolute_path(path)
            .inspect(|absolute_path| log::debug!("musl source: {}", absolute_path.display()))
    }

    /// Returns the path to the MUSL build directory.
    pub fn musl_build(source_directory: &str) -> anyhow::Result<PathBuf> {
        let mut path = PathBuf::from(DIRECTORY_LLVM_TARGET.get().unwrap());
        path.push(source_directory);
        path.push("build");
        crate::utils::absolute_path(path)
            .inspect(|absolute_path| log::debug!("musl build: '{}'", absolute_path.display()))
    }

    /// Returns the path to the LLVM CRT build directory.
    pub fn llvm_build_crt() -> anyhow::Result<PathBuf> {
        let mut path = PathBuf::from(DIRECTORY_LLVM_TARGET.get().unwrap());
        path.push("build-crt");
        crate::utils::absolute_path(path)
            .inspect(|absolute_path| log::debug!("llvm build crt: {}", absolute_path.display()))
    }

    /// Returns the path to the LLVM host build directory.
    pub fn llvm_build_host() -> anyhow::Result<PathBuf> {
        let mut path = PathBuf::from(DIRECTORY_LLVM_TARGET.get().unwrap());
        path.push("build-host");
        crate::utils::absolute_path(path)
            .inspect(|absolute_path| log::debug!("llvm build host: {}", absolute_path.display()))
    }

    /// Returns the path to the LLVM final build directory.
    pub fn llvm_build_final() -> anyhow::Result<PathBuf> {
        let mut path = PathBuf::from(DIRECTORY_LLVM_TARGET.get().unwrap());
        path.push("build-final");
        crate::utils::absolute_path(path)
            .inspect(|absolute_path| log::debug!("llvm build final: {}", absolute_path.display()))
    }

    /// Returns the path to the MUSL target directory.
    pub fn musl_target() -> anyhow::Result<PathBuf> {
        let mut path = PathBuf::from(DIRECTORY_LLVM_TARGET.get().unwrap());
        path.push("target-musl");
        crate::utils::absolute_path(path)
            .inspect(|absolute_path| log::debug!("musl target: {}", absolute_path.display()))
    }

    /// Returns the path to the LLVM CRT target directory.
    pub fn llvm_target_crt() -> anyhow::Result<PathBuf> {
        let mut path = PathBuf::from(DIRECTORY_LLVM_TARGET.get().unwrap());
        path.push("target-crt");
        crate::utils::absolute_path(path)
            .inspect(|absolute_path| log::debug!("llvm crt target: {}", absolute_path.display()))
    }

    /// Returns the path to the LLVM host target directory.
    pub fn llvm_target_host() -> anyhow::Result<PathBuf> {
        let mut path = PathBuf::from(DIRECTORY_LLVM_TARGET.get().unwrap());
        path.push("target-host");
        crate::utils::absolute_path(path)
            .inspect(|absolute_path| log::debug!("llvm host target: {}", absolute_path.display()))
    }

    /// Returns the path to the LLVM final target directory.
    pub fn llvm_target_final() -> anyhow::Result<PathBuf> {
        let mut path = PathBuf::from(DIRECTORY_LLVM_TARGET.get().unwrap());
        path.push("target-final");
        crate::utils::absolute_path(path)
            .inspect(|absolute_path| log::debug!("llvm final target: {}", absolute_path.display()))
    }

    /// Returns the path to the LLVM compiler builtin target directory.
    pub fn llvm_module_compiler_rt() -> anyhow::Result<PathBuf> {
        let mut path = PathBuf::from(Self::DIRECTORY_LLVM_SOURCE);
        path.push("compiler-rt");
        crate::utils::absolute_path(path).inspect(|absolute_path| {
            log::debug!("compiler-rt source dir: {}", absolute_path.display())
        })
    }

    /// Returns the path to the LLVM compiler-rt target directory.
    pub fn llvm_target_compiler_rt() -> anyhow::Result<PathBuf> {
        Self::llvm_target_final()
    }

    /// Returns the path to the LLVM compiler-rt build directory.
    pub fn llvm_build_compiler_rt() -> anyhow::Result<PathBuf> {
        let mut path = PathBuf::from(DIRECTORY_LLVM_TARGET.get().unwrap());
        path.push("build-compiler-rt");
        crate::utils::absolute_path(path).inspect(|absolute_path| {
            log::debug!("llvm compiler-rt build: {}", absolute_path.display())
        })
    }

    /// Returns the path to the LLVM target final bin path.
    ///
    pub fn llvm_target_final_bin(
        target_env: crate::target_env::TargetEnv,
    ) -> anyhow::Result<PathBuf> {
        let mut path = Self::llvm_target_final()?;
        path.push("bin");
        path.push(format!("{target_env}"));
        crate::utils::absolute_path(path).inspect(|absolute_path| {
            log::debug!("llvm target final bin: {}", absolute_path.display())
        })
    }
}
