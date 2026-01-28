//! Solidity to PolkaVM compiler binary.

use std::{io::Write, path::PathBuf};

use clap::error::ErrorKind;
use resolc::Process;
use revive_common::{deserialize_from_str, EVMVersion, EXIT_CODE_FAILURE, EXIT_CODE_SUCCESS};
use revive_llvm_context::{initialize_llvm, DebugConfig, OptimizerSettings, PolkaVMTarget};
use revive_solc_json_interface::{
    ResolcWarning, SolcStandardJsonInputSettingsPolkaVMMemory,
    SolcStandardJsonInputSettingsSelection, SolcStandardJsonOutput, SolcStandardJsonOutputError,
};

use self::arguments::Arguments;

pub mod arguments;

#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> anyhow::Result<()> {
    let arguments = <Arguments as clap::Parser>::try_parse().inspect_err(|error| {
        if let ErrorKind::DisplayHelp = error.kind() {
            let _ = error.print();
            std::process::exit(EXIT_CODE_SUCCESS);
        }
    })?;

    let is_standard_json = arguments.standard_json.is_some();
    let mut messages = arguments.validate();
    if messages.iter().all(|error| !error.is_error()) {
        if !is_standard_json {
            std::io::stderr()
                .write_all(
                    messages
                        .drain(..)
                        .map(|error| error.to_string())
                        .collect::<Vec<String>>()
                        .join("\n")
                        .as_bytes(),
                )
                .expect("Stderr writing error");
        }
        if let Err(error) = main_inner(arguments, &mut messages) {
            messages.push(SolcStandardJsonOutputError::new_error(error, None, None));
        }
    }

    if is_standard_json {
        let output = SolcStandardJsonOutput::new_with_messages(messages);
        output.write_and_exit(SolcStandardJsonInputSettingsSelection::default());
    }

    std::io::stderr()
        .write_all(
            messages
                .iter()
                .map(|error| error.to_string())
                .collect::<Vec<String>>()
                .join("\n")
                .as_bytes(),
        )
        .expect("Stderr writing error");

    std::process::exit(
        if messages.iter().any(SolcStandardJsonOutputError::is_error) {
            EXIT_CODE_FAILURE
        } else {
            EXIT_CODE_SUCCESS
        },
    );
}

fn main_inner(
    arguments: Arguments,
    messages: &mut Vec<SolcStandardJsonOutputError>,
) -> anyhow::Result<()> {
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
        .stack_size(resolc::RAYON_WORKER_STACK_SIZE)
        .build_global()
        .expect("Thread pool configuration failure");

    if arguments.recursive_process {
        let input_json = std::io::read_to_string(std::io::stdin())
            .map_err(|error| anyhow::anyhow!("Stdin reading error: {error}"))?;
        let input: resolc::ProcessInput = deserialize_from_str(input_json.as_str())
            .map_err(|error| anyhow::anyhow!("Stdin parsing error: {error}"))?;

        initialize_llvm(
            PolkaVMTarget::PVM,
            resolc::DEFAULT_EXECUTABLE_NAME,
            &input.llvm_arguments,
        );

        #[cfg(target_os = "emscripten")]
        {
            return resolc::WorkerProcess::run(input);
        }
        #[cfg(not(target_os = "emscripten"))]
        {
            return resolc::NativeProcess::run(input);
        }
    }

    initialize_llvm(
        PolkaVMTarget::PVM,
        resolc::DEFAULT_EXECUTABLE_NAME,
        &arguments.llvm_arguments,
    );

    let debug_config = match arguments.debug_output_directory {
        Some(ref debug_output_directory) => {
            std::fs::create_dir_all(debug_output_directory.as_path())?;
            DebugConfig::new(
                Some(debug_output_directory.to_owned()),
                arguments.emit_source_debug_info,
            )
        }
        None => DebugConfig::new(None, arguments.emit_source_debug_info),
    };

    let (input_files, remappings) = arguments.split_input_files_and_remappings()?;

    let suppressed_warnings = ResolcWarning::try_from_strings(
        arguments.suppress_warnings.unwrap_or_default().as_slice(),
    )?;

    let solc = {
        #[cfg(target_os = "emscripten")]
        {
            resolc::SoljsonCompiler {}
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
        Some(evm_version) => Some(EVMVersion::try_from(evm_version.as_str())?),
        None => None,
    };

    let mut optimizer_settings = OptimizerSettings::try_from_cli(arguments.optimization)?;
    optimizer_settings.is_verify_each_enabled = arguments.llvm_verify_each;
    optimizer_settings.is_debug_logging_enabled = arguments.llvm_debug_logging;

    let memory_config = SolcStandardJsonInputSettingsPolkaVMMemory::new(
        Some(arguments.heap_size),
        Some(arguments.stack_size),
    );

    let build = if arguments.yul {
        resolc::yul(
            &solc,
            input_files.as_slice(),
            arguments.libraries.as_slice(),
            arguments.metadata_hash,
            messages,
            optimizer_settings,
            debug_config,
            &arguments.llvm_arguments,
            memory_config,
        )
    } else if let Some(standard_json) = arguments.standard_json {
        resolc::standard_json(
            &solc,
            arguments.metadata_hash,
            messages,
            standard_json.map(PathBuf::from),
            arguments.base_path,
            arguments.include_paths,
            arguments.allow_paths,
            debug_config,
            arguments.detect_missing_libraries,
        )?;
        return Ok(());
    } else if let Some(format) = arguments.combined_json {
        resolc::combined_json(
            &solc,
            input_files.as_slice(),
            arguments.libraries.as_slice(),
            arguments.metadata_hash,
            messages,
            evm_version,
            format,
            !arguments.disable_solc_optimizer,
            optimizer_settings,
            arguments.base_path,
            arguments.include_paths,
            arguments.allow_paths,
            remappings,
            suppressed_warnings,
            debug_config,
            arguments.output_directory,
            arguments.overwrite,
            arguments.llvm_arguments,
            memory_config,
        )?;
        return Ok(());
    } else if arguments.link {
        return resolc::link(arguments.inputs, arguments.libraries);
    } else {
        resolc::standard_output(
            &solc,
            input_files.as_slice(),
            arguments.libraries.as_slice(),
            arguments.metadata_hash,
            messages,
            evm_version,
            !arguments.disable_solc_optimizer,
            optimizer_settings,
            arguments.base_path,
            arguments.include_paths,
            arguments.allow_paths,
            remappings,
            suppressed_warnings,
            debug_config,
            arguments.llvm_arguments,
            memory_config,
        )
    }?;

    if let Some(output_directory) = arguments.output_directory {
        build.write_to_directory(
            &output_directory,
            arguments.output_metadata,
            arguments.output_assembly,
            arguments.output_binary,
            arguments.overwrite,
        )?;
    } else {
        build.write_to_terminal(
            arguments.output_metadata,
            arguments.output_assembly,
            arguments.output_binary,
        )?;
    }

    Ok(())
}
