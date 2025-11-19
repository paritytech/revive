//! The revive ELF object linker library.

use std::{ffi::CString, fs, path::PathBuf, sync::Mutex};

use lld_sys::LLDELFLink;
use tempfile::TempDir;

use revive_builtins::COMPILER_RT;

static GUARD: Mutex<()> = Mutex::new(());

/// The revive ELF object linker.
pub struct ElfLinker {
    temporary_directory: TempDir,
    output_path: PathBuf,
    object_path: PathBuf,
    symbols_path: PathBuf,
    linker_script_path: PathBuf,
}

impl ElfLinker {
    const LINKER_SCRIPT: &str = r#"
SECTIONS {
    .text : { KEEP(*(.text.polkavm_export)) *(.text .text.*) }
}"#;

    const BUILTINS_ARCHIVE_FILE: &str = "libclang_rt.builtins-riscv64.a";
    const BUILTINS_LIB_NAME: &str = "clang_rt.builtins-riscv64";

    /// The setup routine prepares a temporary working directory.
    pub fn setup() -> anyhow::Result<Self> {
        let temporary_directory = TempDir::new()?;
        let object_path = temporary_directory.path().join("obj.o");
        let output_path = temporary_directory.path().join("out.o");
        let symbols_path = temporary_directory.path().join("sym.o");
        let linker_script_path = temporary_directory.path().join("linker.ld");

        fs::write(&linker_script_path, Self::LINKER_SCRIPT)
            .map_err(|message| anyhow::anyhow!("{message} {linker_script_path:?}",))?;

        let compiler_rt_path = temporary_directory.path().join(Self::BUILTINS_ARCHIVE_FILE);
        fs::write(&compiler_rt_path, COMPILER_RT)
            .map_err(|message| anyhow::anyhow!("{message} {compiler_rt_path:?}"))?;

        Ok(Self {
            temporary_directory,
            output_path,
            object_path,
            symbols_path,
            linker_script_path,
        })
    }

    /// Link `input` with `symbols` and the `compiler_rt` via `LLD`.
    pub fn link<T: AsRef<[u8]>>(self, input: T, symbols: T) -> anyhow::Result<Vec<u8>> {
        fs::write(&self.object_path, input)
            .map_err(|message| anyhow::anyhow!("{message} {:?}", self.object_path))?;

        fs::write(&self.symbols_path, symbols)
            .map_err(|message| anyhow::anyhow!("{message} {:?}", self.symbols_path))?;

        if lld(self
            .create_arguments()
            .into_iter()
            .map(|v| v.to_string())
            .collect())
        {
            return Err(anyhow::anyhow!("ld.lld failed"));
        }

        Ok(fs::read(&self.output_path)?)
    }

    /// The argument creation helper function.
    fn create_arguments(&self) -> Vec<String> {
        [
            "ld.lld",
            "--error-limit=0",
            "--relocatable",
            "--emit-relocs",
            "--relax",
            "--unique",
            "--gc-sections",
            self.linker_script_path.to_str().expect("should be utf8"),
            "-o",
            self.output_path.to_str().expect("should be utf8"),
            self.object_path.to_str().expect("should be utf8"),
            self.symbols_path.to_str().expect("should be utf8"),
            "--library-path",
            self.temporary_directory
                .path()
                .to_str()
                .expect("should be utf8"),
            "--library",
            Self::BUILTINS_LIB_NAME,
        ]
        .iter()
        .map(ToString::to_string)
        .collect()
    }
}

/// The thread-safe LLD helper function.
fn lld(arguments: Vec<String>) -> bool {
    let c_strings = arguments
        .into_iter()
        .map(|arg| CString::new(arg).expect("ld.lld args should not contain null bytes"))
        .collect::<Vec<_>>();

    let args: Vec<*const libc::c_char> = c_strings.iter().map(|arg| arg.as_ptr()).collect();

    let _lock = GUARD.lock().expect("ICE: linker mutex should not poison");
    unsafe { LLDELFLink(args.as_ptr(), args.len()) == 0 }
}
