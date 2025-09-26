//! The Solidity to PolkaVM compiler deploy time linking library.
//!
//! # Deploy time linking
//!
//! At compile time, factory dependencies and library addresses
//! are declared but not necessarily defined.
//!
//! `resolc` will emit raw ELF objects for any contract requiring
//! deploy time linking using the `--link` flag.
//!
//! # Internals
//!
//! After all contracts have been built successfully, the compiler
//! tries to link the resulting raw ELF object files into PVM blobs.
//! This fails if any library address symbols are unknown at compile
//! time (which is better known in Solidity as the so called "deploy
//! time linking" feature). Since factory dependency symbols can be
//! resolved only after the the final PVM blob linking step, missing
//! libraries may further lead to unresolved factory dependencies.

use std::collections::BTreeMap;

use revive_common::{ObjectFormat, EXTENSION_POLKAVM_BINARY};
use revive_llvm_context::{polkavm_hash, polkavm_link};
use revive_solc_json_interface::SolcStandardJsonInputSettingsLibraries;

/// The Solidity to PolkaVM compiler deploy time linking outputs.
pub struct Output {
    /// The linked objects.
    pub linked: BTreeMap<String, Vec<u8>>,
    /// The unlinked objects.
    pub unlinked: Vec<(String, Vec<u8>)>,
}

impl Output {
    /// Try linking given `libraries` into given `bytecodes`.
    ///
    /// Bytecodes failing to fully resolve end up in [Output::unlinked].
    pub fn try_from(
        bytecodes: &BTreeMap<String, Vec<u8>>,
        libraries: &[String],
    ) -> anyhow::Result<Self> {
        let linker_symbols =
            SolcStandardJsonInputSettingsLibraries::try_from(libraries)?.as_linker_symbols()?;

        let mut linked = BTreeMap::default();
        let mut unlinked = Vec::default();
        let mut factory_dependencies = BTreeMap::default();

        for (path, bytecode) in bytecodes {
            match ObjectFormat::try_from(bytecode.as_slice()) {
                Ok(ObjectFormat::ELF) => unlinked.push((path.clone(), bytecode.clone())),
                Ok(ObjectFormat::PVM) => {
                    factory_dependencies
                        .insert(factory_dependency_symbol(path), polkavm_hash(bytecode));
                }
                Err(error) => anyhow::bail!("{path}: {error}"),
            }
        }

        loop {
            let mut linked_counter = 0;
            let mut remaining_objects = Vec::new();
            for (path, bytecode_buffer) in unlinked.drain(..) {
                let (linked_bytecode, object_format) = polkavm_link(
                    &bytecode_buffer,
                    &linker_symbols,
                    &factory_dependencies,
                    true,
                )?;
                match object_format {
                    ObjectFormat::ELF => remaining_objects.push((path, linked_bytecode)),
                    ObjectFormat::PVM => {
                        factory_dependencies.insert(
                            factory_dependency_symbol(&path),
                            polkavm_hash(&linked_bytecode),
                        );
                        linked.insert(path, linked_bytecode);
                        linked_counter += 1;
                    }
                }
            }
            unlinked = remaining_objects;
            if linked_counter == 0 {
                break;
            }
        }

        Ok(Self { linked, unlinked })
    }
}

fn factory_dependency_symbol(path: &str) -> String {
    path.trim_end_matches(&format!(".{EXTENSION_POLKAVM_BINARY}"))
        .to_string()
}
