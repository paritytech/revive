//! The revive ELF object linker library.

use std::{ffi::CString, fs, io, path::PathBuf, sync::Mutex, thread, time::Duration};

use lld_sys::LLDELFLink;
use tempfile::TempDir;

use revive_builtins::COMPILER_RT;

static GUARD: Mutex<()> = Mutex::new(());

/// Bounded so that a persistent permission problem still surfaces instead of retrying forever.
const TEMPORARY_DIRECTORY_RETRIES: usize = 3;
const TEMPORARY_DIRECTORY_RETRY_PAUSE: Duration = Duration::from_millis(1);

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
        let temporary_directory = retry_when_access_is_denied(TempDir::new)?;
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

/// Windows reports a name clash with a directory that is still being deleted as access denied
/// rather than as already existing, which `tempfile` does not retry. Each retry uses a new
/// random name with a short growing delay.
fn retry_when_access_is_denied<T>(mut create: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut retries = 0;
    loop {
        match create() {
            Err(error)
                if error.kind() == io::ErrorKind::PermissionDenied
                    && retries < TEMPORARY_DIRECTORY_RETRIES =>
            {
                retries += 1;
                thread::sleep(TEMPORARY_DIRECTORY_RETRY_PAUSE * retries as u32);
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::{retry_when_access_is_denied, TEMPORARY_DIRECTORY_RETRIES};

    #[test]
    fn retries_when_access_is_denied_once() {
        let mut calls = 0;
        retry_when_access_is_denied(|| {
            calls += 1;
            if calls == 1 {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                Ok(())
            }
        })
        .expect("an access permission denial should be retried");
        assert_eq!(calls, 2);
    }

    #[test]
    fn gives_up_when_access_stays_denied() {
        let mut calls = 0;
        let error = retry_when_access_is_denied(|| -> io::Result<()> {
            calls += 1;
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(calls, TEMPORARY_DIRECTORY_RETRIES + 1);
    }

    #[test]
    fn returns_non_permission_denied_errors_without_retrying() {
        let mut calls = 0;
        let error = retry_when_access_is_denied(|| -> io::Result<()> {
            calls += 1;
            Err(io::Error::from(io::ErrorKind::NotFound))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(calls, 1);
    }
}
