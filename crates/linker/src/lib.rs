use std::{env, ffi::CString, fs, path::PathBuf};

use lld_sys::LLDELFLink;
use tempfile::TempDir;

use revive_builtins::COMPILER_RT;

pub struct Linker {
    temporary_directory: TempDir,
    output_path: PathBuf,
    object_path: PathBuf,
    symbols_path: PathBuf,
    linker_script_path: PathBuf,
    link_compiler_rt: bool,
}

impl Linker {
    const LINKER_SCRIPT: &str = r#"
SECTIONS {
    .text : { KEEP(*(.text.polkavm_export)) *(.text .text.*) }
}"#;

    const BUILTINS_ARCHIVE_FILE: &str = "libclang_rt.builtins-riscv64.a";
    const BUILTINS_LIB_NAME: &str = "clang_rt.builtins-riscv64";

    pub fn setup(link_compiler_rt: bool) -> anyhow::Result<Self> {
        let temporary_directory = TempDir::new()?;
        let output_path = temporary_directory.path().join("out.so");
        let object_path = temporary_directory.path().join("out.o");
        let symbols_path = temporary_directory.path().join("sym.o");
        let linker_script_path = temporary_directory.path().join("linker.ld");

        fs::write(&linker_script_path, Self::LINKER_SCRIPT)
            .map_err(|message| anyhow::anyhow!("{message} {linker_script_path:?}",))?;

        if link_compiler_rt {
            let compiler_rt_path = temporary_directory.path().join(Self::BUILTINS_ARCHIVE_FILE);
            fs::write(&compiler_rt_path, COMPILER_RT)
                .map_err(|message| anyhow::anyhow!("{message} {compiler_rt_path:?}"))?;
        }

        Ok(Self {
            temporary_directory,
            output_path,
            object_path,
            symbols_path,
            linker_script_path,
            link_compiler_rt,
        })
    }

    pub fn link<T: AsRef<[u8]>>(self, input: T, symbols: Option<T>) -> anyhow::Result<Vec<u8>> {
        fs::write(&self.object_path, input)
            .map_err(|message| anyhow::anyhow!("{message} {:?}", self.object_path))?;

        if let Some(symbols) = symbols.as_ref() {
            fs::write(&self.symbols_path, symbols)
                .map_err(|message| anyhow::anyhow!("{message} {:?}", self.symbols_path))?;
        }

        //if env::var("PVM_LINKER_DUMP_OBJ").is_ok() {
        //    fs::copy(&object_path, "/tmp/out.o")?;
        //}
        let arguments = self.create_arguments(symbols.is_some()).into_iter();
        if invoke_lld(arguments) {
            return Err(anyhow::anyhow!("ld.lld failed"));
        }

        Ok(fs::read(&self.output_path)?)
    }

    fn create_arguments(&self, link_symbols: bool) -> Vec<String> {
        let mut arguments = vec![
            "ld.lld",
            "--error-limit=0",
            "--relocatable",
            "--emit-relocs",
            "--no-relax",
            "--unique",
            "--gc-sections",
            self.linker_script_path.to_str().expect("should be utf8"),
            "-o",
            self.output_path.to_str().expect("should be utf8"),
            self.object_path.to_str().expect("should be utf8"),
        ];

        if link_symbols {
            arguments.push(self.symbols_path.to_str().expect("should be utf8"));
        }

        if self.link_compiler_rt {
            arguments.extend(&[
                "--library-path",
                self.temporary_directory
                    .path()
                    .to_str()
                    .expect("should be utf8"),
                "--library",
                Self::BUILTINS_LIB_NAME,
            ]);
        }

        arguments.iter().map(ToString::to_string).collect()
    }
}

fn invoke_lld<T: Into<Vec<u8>>>(arguments: impl Iterator<Item = T>) -> bool {
    let c_strings = arguments
        .map(|arg| CString::new(arg).expect("ld.lld args should not contain null bytes"))
        .collect::<Vec<_>>();

    let args: Vec<*const libc::c_char> = c_strings.iter().map(|arg| arg.as_ptr()).collect();

    unsafe { LLDELFLink(args.as_ptr(), args.len()) == 0 }
}

pub fn polkavm_linker<T: AsRef<[u8]>>(code: T, strip_binary: bool) -> anyhow::Result<Vec<u8>> {
    let mut config = polkavm_linker::Config::default();
    config.set_strip(strip_binary);
    config.set_optimize(true);

    polkavm_linker::program_from_elf(config, code.as_ref())
        .map_err(|reason| anyhow::anyhow!("polkavm linker failed: {}", reason))
}
