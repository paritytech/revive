use std::collections::BTreeMap;

use revive_common::ObjectFormat;
use revive_llvm_context::{polkavm_hash, polkavm_link};
use revive_solc_json_interface::SolcStandardJsonInputSettingsLibraries;

/// The linker results.
#[derive(Default)]
pub struct Linker {
    /// The linked objects.
    pub linked: BTreeMap<String, Vec<u8>>,
    /// The unlinked objects.
    pub unlinked: Vec<(String, Vec<u8>)>,
}

impl Linker {
    /// Try linking given `bytecodes` with given `libraries`.
    pub fn try_link(
        bytecodes: &BTreeMap<String, Vec<u8>>,
        libraries: &[String],
    ) -> anyhow::Result<Self> {
        let mut linker = Self::default();
        let linker_symbols =
            SolcStandardJsonInputSettingsLibraries::try_from(libraries)?.as_linker_symbols()?;
        let mut factory_dependencies = BTreeMap::new();

        for (path, bytecode) in bytecodes {
            if ObjectFormat::try_from(bytecode.as_slice())
                .map_err(|error| anyhow::anyhow!("{path}: {error}"))?
                == ObjectFormat::ELF
            {
                linker.unlinked.push((path.clone(), bytecode.to_owned()));
                continue;
            }

            let hash = polkavm_hash(bytecode);
            factory_dependencies.insert(path.clone(), hash);
        }

        loop {
            let mut linked_counter = 0;
            let mut remaining_objects = Vec::new();
            for (path, bytecode_buffer) in linker.unlinked.drain(..) {
                let (linked_bytecode, object_format) =
                    polkavm_link(&bytecode_buffer, &linker_symbols, &factory_dependencies)?;
                match object_format {
                    ObjectFormat::ELF => {
                        remaining_objects.push((path, linked_bytecode));
                    }
                    ObjectFormat::PVM => {
                        factory_dependencies.insert(path.clone(), polkavm_hash(&linked_bytecode));
                        linker.linked.insert(path.clone(), linked_bytecode);
                        linked_counter += 1;
                    }
                }
            }
            linker.unlinked = remaining_objects;
            if linked_counter == 0 {
                break;
            }
        }

        Ok(linker)
    }
}
