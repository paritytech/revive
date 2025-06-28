//! Solidity to PolkaVM compiler binary.

pub mod arguments;

use std::io::Write;
use std::str::FromStr;

use resolc::Process;

use self::arguments::Arguments;

#[cfg(feature = "parallel")]
/// The rayon worker stack size.
const RAYON_WORKER_STACK_SIZE: usize = 16 * 1024 * 1024;

#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> anyhow::Result<()> {
    std::process::exit(match main_inner() {
        Ok(()) => revive_common::EXIT_CODE_SUCCESS,
        Err(error) => {
            writeln!(std::io::stderr(), "{error}")?;
            revive_common::EXIT_CODE_FAILURE
        }
    })
}

fn main_inner() -> anyhow::Result<()> {
    let arguments = <Arguments as clap::Parser>::try_parse()?;
    arguments.validate()?;

    if arguments.version {
        writeln!(
            std::io::stdout(),
            "{} version {}",
            env!("CARGO_PKG_DESCRIPTION"),
            resolc::ResolcVersion::default().long
        )?;
        return Ok(());
    }

    if arguments.supported_solc_versions {
        writeln!(
            std::io::stdout(),
            ">={},<={}",
            resolc::SolcFirstSupportedVersion,
            resolc::SolcLastSupportedVersion,
        )?;
        return Ok(());
    }

    #[cfg(feature = "parallel")]
    rayon::ThreadPoolBuilder::new()
        .stack_size(RAYON_WORKER_STACK_SIZE)
        .build_global()
        .expect("Thread pool configuration failure");

    if arguments.recursive_process {
        #[cfg(debug_assertions)]
        if let Some(fname) = arguments.recursive_process_input {
            let mut infile = std::fs::File::open(fname)?;
            #[cfg(target_os = "emscripten")]
            {
                return resolc::WorkerProcess::run(Some(&mut infile));
            }
            #[cfg(not(target_os = "emscripten"))]
            {
                return resolc::NativeProcess::run(Some(&mut infile));
            }
        }
        #[cfg(target_os = "emscripten")]
        {
            return resolc::WorkerProcess::run(None);
        }
        #[cfg(not(target_os = "emscripten"))]
        {
            return resolc::NativeProcess::run(None);
        }
    }

    let debug_config = match arguments.debug_output_directory {
        Some(ref debug_output_directory) => {
            std::fs::create_dir_all(debug_output_directory.as_path())?;
            revive_llvm_context::DebugConfig::new(
                Some(debug_output_directory.to_owned()),
                arguments.emit_source_debug_info,
            )
        }
        None => revive_llvm_context::DebugConfig::new(None, arguments.emit_source_debug_info),
    };

    let (input_files, remappings) = arguments.split_input_files_and_remappings()?;

    let suppressed_warnings = match arguments.suppress_warnings {
        Some(warnings) => Some(revive_solc_json_interface::ResolcWarning::try_from_strings(
            warnings.as_slice(),
        )?),
        None => None,
    };

    let mut solc = {
        #[cfg(target_os = "emscripten")]
        {
            resolc::SoljsonCompiler
        }

        #[cfg(not(target_os = "emscripten"))]
        {
            resolc::SolcCompiler::new(
                arguments
                    .solc
                    .unwrap_or_else(|| resolc::SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned()),
            )?
        }
    };

    let evm_version = match arguments.evm_version {
        Some(evm_version) => Some(revive_common::EVMVersion::try_from(evm_version.as_str())?),
        None => None,
    };

    let mut optimizer_settings = match arguments.optimization {
        Some(mode) => revive_llvm_context::OptimizerSettings::try_from_cli(mode)?,
        None => revive_llvm_context::OptimizerSettings::size(),
    };
    if arguments.fallback_to_optimizing_for_size {
        optimizer_settings.enable_fallback_to_size();
    }
    optimizer_settings.is_verify_each_enabled = arguments.llvm_verify_each;
    optimizer_settings.is_debug_logging_enabled = arguments.llvm_debug_logging;

    let include_metadata_hash = match arguments.metadata_hash {
        Some(metadata_hash) => {
            let metadata =
                revive_solc_json_interface::SolcStandardJsonInputSettingsMetadataHash::from_str(
                    metadata_hash.as_str(),
                )?;
            metadata != revive_solc_json_interface::SolcStandardJsonInputSettingsMetadataHash::None
        }
        None => true,
    };

    let memory_config = revive_solc_json_interface::SolcStandardJsonInputSettingsPolkaVMMemory::new(
        arguments.heap_size,
        arguments.stack_size,
    );

    let build = if arguments.yul {
        resolc::yul(
            input_files.as_slice(),
            &mut solc,
            optimizer_settings,
            include_metadata_hash,
            debug_config,
            &arguments.llvm_arguments,
            memory_config,
        )
    } else if arguments.llvm_ir {
        resolc::llvm_ir(
            input_files.as_slice(),
            optimizer_settings,
            include_metadata_hash,
            debug_config,
            &arguments.llvm_arguments,
            memory_config,
        )
    } else if arguments.standard_json {
        resolc::standard_json(
            &mut solc,
            arguments.detect_missing_libraries,
            arguments.base_path,
            arguments.include_paths,
            arguments.allow_paths,
            debug_config,
            &arguments.llvm_arguments,
        )?;
        return Ok(());
    } else if let Some(format) = arguments.combined_json {
        resolc::combined_json(
            format,
            input_files.as_slice(),
            arguments.libraries,
            &mut solc,
            evm_version,
            !arguments.disable_solc_optimizer,
            optimizer_settings,
            include_metadata_hash,
            arguments.base_path,
            arguments.include_paths,
            arguments.allow_paths,
            remappings,
            suppressed_warnings,
            debug_config,
            arguments.output_directory,
            arguments.overwrite,
            &arguments.llvm_arguments,
            memory_config,
        )?;
        return Ok(());
    } else {
        resolc::standard_output(
            input_files.as_slice(),
            arguments.libraries,
            &mut solc,
            evm_version,
            !arguments.disable_solc_optimizer,
            optimizer_settings,
            include_metadata_hash,
            arguments.base_path,
            arguments.include_paths,
            arguments.allow_paths,
            remappings,
            suppressed_warnings,
            debug_config,
            &arguments.llvm_arguments,
            memory_config,
        )
    }?;

    if let Some(output_directory) = arguments.output_directory {
        std::fs::create_dir_all(&output_directory)?;

        build.write_to_directory(
            &output_directory,
            arguments.output_assembly,
            arguments.output_binary,
            arguments.overwrite,
        )?;

        writeln!(
            std::io::stderr(),
            "Compiler run successful. Artifact(s) can be found in directory {output_directory:?}."
        )?;
    } else if arguments.output_assembly || arguments.output_binary {
        for (path, contract) in build.contracts.into_iter() {
            if arguments.output_assembly {
                let assembly_text = contract.build.assembly_text;

                writeln!(
                    std::io::stdout(),
                    "Contract `{path}` assembly:\n\n{assembly_text}"
                )?;
            }
            if arguments.output_binary {
                writeln!(
                    std::io::stdout(),
                    "Contract `{}` bytecode: 0x{}",
                    path,
                    hex::encode(contract.build.bytecode)
                )?;
            }
        }
    } else {
        writeln!(
            std::io::stderr(),
            "Compiler run successful. No output requested. Use --asm and --bin flags."
        )?;
    }

    Ok(())
}
