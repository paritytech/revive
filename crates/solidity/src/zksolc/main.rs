//! Solidity to PolkaVM compiler binary.

pub mod arguments;

use std::str::FromStr;

use self::arguments::Arguments;

use polkavm_common::program::ProgramBlob;
use polkavm_disassembler::{Disassembler, DisassemblyFormat};

/// The rayon worker stack size.
const RAYON_WORKER_STACK_SIZE: usize = 16 * 1024 * 1024;

#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// The application entry point.
fn main() {
    std::process::exit(match main_inner() {
        Ok(()) => revive_common::EXIT_CODE_SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            revive_common::EXIT_CODE_FAILURE
        }
    })
}

/// The auxiliary `main` function to facilitate the `?` error conversion operator.
fn main_inner() -> anyhow::Result<()> {
    let arguments = Arguments::new();
    arguments.validate()?;

    if arguments.version {
        println!(
            "{} v{} (LLVM build {:?})",
            env!("CARGO_PKG_DESCRIPTION"),
            env!("CARGO_PKG_VERSION"),
            inkwell::support::get_llvm_version()
        );
        return Ok(());
    }

    rayon::ThreadPoolBuilder::new()
        .stack_size(RAYON_WORKER_STACK_SIZE)
        .build_global()
        .expect("Thread pool configuration failure");
    inkwell::support::enable_llvm_pretty_stack_trace();
    revive_llvm_context::initialize_target(revive_llvm_context::Target::PVM); // TODO: pass from CLI

    if arguments.recursive_process {
        return revive_solidity::run_process();
    }

    let debug_config = match arguments.debug_output_directory {
        Some(ref debug_output_directory) => {
            std::fs::create_dir_all(debug_output_directory.as_path())?;
            Some(revive_llvm_context::DebugConfig::new(
                debug_output_directory.to_owned(),
            ))
        }
        None => None,
    };

    let (input_files, remappings) = arguments.split_input_files_and_remappings()?;

    let suppressed_warnings = match arguments.suppress_warnings {
        Some(warnings) => Some(revive_solidity::Warning::try_from_strings(
            warnings.as_slice(),
        )?),
        None => None,
    };

    let mut solc = revive_solidity::SolcCompiler::new(
        arguments
            .solc
            .unwrap_or_else(|| revive_solidity::SolcCompiler::DEFAULT_EXECUTABLE_NAME.to_owned()),
    )?;

    let evm_version = match arguments.evm_version {
        Some(evm_version) => Some(revive_common::EVMVersion::try_from(evm_version.as_str())?),
        None => None,
    };

    let mut optimizer_settings = match arguments.optimization {
        Some(mode) => revive_llvm_context::OptimizerSettings::try_from_cli(mode)?,
        None => revive_llvm_context::OptimizerSettings::cycles(),
    };
    if arguments.fallback_to_optimizing_for_size {
        optimizer_settings.enable_fallback_to_size();
    }
    if arguments.disable_system_request_memoization {
        optimizer_settings.disable_system_request_memoization();
    }
    optimizer_settings.is_verify_each_enabled = arguments.llvm_verify_each;
    optimizer_settings.is_debug_logging_enabled = arguments.llvm_debug_logging;

    let include_metadata_hash = match arguments.metadata_hash {
        Some(metadata_hash) => {
            let metadata =
                revive_llvm_context::PolkaVMMetadataHash::from_str(metadata_hash.as_str())?;
            metadata != revive_llvm_context::PolkaVMMetadataHash::None
        }
        None => true,
    };

    let build = if arguments.yul {
        revive_solidity::yul(
            input_files.as_slice(),
            &mut solc,
            optimizer_settings,
            arguments.is_system_mode,
            include_metadata_hash,
            debug_config,
        )
    } else if arguments.llvm_ir {
        revive_solidity::llvm_ir(
            input_files.as_slice(),
            optimizer_settings,
            arguments.is_system_mode,
            include_metadata_hash,
            debug_config,
        )
    } else if arguments.zkasm {
        revive_solidity::zkasm(input_files.as_slice(), include_metadata_hash, debug_config)
    } else if arguments.standard_json {
        revive_solidity::standard_json(
            &mut solc,
            arguments.detect_missing_libraries,
            arguments.force_evmla,
            arguments.is_system_mode,
            arguments.base_path,
            arguments.include_paths,
            arguments.allow_paths,
            debug_config,
        )?;
        return Ok(());
    } else if let Some(format) = arguments.combined_json {
        revive_solidity::combined_json(
            format,
            input_files.as_slice(),
            arguments.libraries,
            &mut solc,
            evm_version,
            !arguments.disable_solc_optimizer,
            optimizer_settings,
            arguments.force_evmla,
            arguments.is_system_mode,
            include_metadata_hash,
            arguments.base_path,
            arguments.include_paths,
            arguments.allow_paths,
            remappings,
            suppressed_warnings,
            debug_config,
            arguments.output_directory,
            arguments.overwrite,
        )?;
        return Ok(());
    } else {
        revive_solidity::standard_output(
            input_files.as_slice(),
            arguments.libraries,
            &mut solc,
            evm_version,
            !arguments.disable_solc_optimizer,
            optimizer_settings,
            arguments.force_evmla,
            arguments.is_system_mode,
            include_metadata_hash,
            arguments.base_path,
            arguments.include_paths,
            arguments.allow_paths,
            remappings,
            suppressed_warnings,
            debug_config,
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

        eprintln!(
            "Compiler run successful. Artifact(s) can be found in directory {output_directory:?}."
        );
    } else if arguments.output_assembly || arguments.output_binary {
        for (path, contract) in build.contracts.into_iter() {
            let bytescode = contract.build.bytecode;

            if arguments.output_assembly {
                let program_blob = match ProgramBlob::parse(bytescode.as_slice()) {
                    Ok(blob) => blob,
                    Err(error) => {
                        anyhow::bail!("Failed to parse program blob: {}", error);
                    }
                };

                let disassembler_object =
                    Disassembler::new(&program_blob, DisassemblyFormat::Guest)?;

                let mut disassembled_code = Vec::new();
                disassembler_object.disassemble_into(&mut disassembled_code)?;

                let assembly_text = String::from_utf8(disassembled_code)?;

                println!("Contract `{}` assembly:\n\n{}", path, assembly_text);
            }
            if arguments.output_binary {
                println!("Contract `{}` bytecode: 0x{}", path, hex::encode(bytescode));
            }
        }
    } else {
        eprintln!("Compiler run successful. No output requested. Use --asm and --bin flags.");
    }

    Ok(())
}
