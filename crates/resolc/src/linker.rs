use std::collections::BTreeMap;

use revive_common::ObjectFormat;
use revive_llvm_context::{polkavm_hash, polkavm_link};
use revive_solc_json_interface::SolcStandardJsonInputSettingsLibraries;

pub fn link(
    bytecodes: &BTreeMap<String, String>,
    libraries: &[String],
) -> anyhow::Result<BTreeMap<String, (String, String)>> {
    let linker_symbols =
        SolcStandardJsonInputSettingsLibraries::try_from(libraries)?.as_linker_symbols()?;
    let mut unlinked_objects = Vec::new();
    let mut factory_dependencies = BTreeMap::new();
    let mut ignored = BTreeMap::new();
    let mut linked = BTreeMap::new();

    let bytecode_binary = bytecodes
        .iter()
        .map(|(path, string)| {
            let string_stripped = string.strip_prefix("0x").unwrap_or(string.as_str());
            let bytecode = hex::decode(string_stripped).map_err(|error| {
                anyhow::anyhow!("Object `{path}` hexadecimal string decoding: {error}")
            })?;
            Ok((path.to_owned(), bytecode))
        })
        .collect::<anyhow::Result<BTreeMap<String, Vec<u8>>>>()?;

    for (path, bytecode_string) in bytecodes {
        let bytecode = bytecode_binary.get(path.as_str()).expect("Always exists");

        if ObjectFormat::try_from(bytecode.as_slice())
            .map_err(|error| anyhow::anyhow!("{path}: {error}"))?
            == ObjectFormat::ELF
        {
            unlinked_objects.push((path, bytecode.to_owned()));
            continue;
        }

        let hash = polkavm_hash(bytecode);
        ignored.insert(path.clone(), (bytecode_string, hex::encode(hash)));
        factory_dependencies.insert(path.clone(), hash);
    }

    loop {
        let mut linked_counter = 0;
        let mut remaining_objects = Vec::new();
        for (path, bytecode_buffer) in unlinked_objects.drain(..) {
            let (linked_bytecode, object_format) =
                polkavm_link(&bytecode_buffer, &linker_symbols, &factory_dependencies)?;
            match object_format {
                ObjectFormat::ELF => {
                    remaining_objects.push((path, linked_bytecode));
                }
                ObjectFormat::PVM => {
                    let bytecode = hex::encode(&linked_bytecode);
                    let hash = polkavm_hash(&linked_bytecode);

                    linked.insert(path.clone(), (bytecode, hex::encode(hash)));

                    factory_dependencies.insert(path.clone(), hash);
                    linked_counter += 1;
                }
            }
        }
        unlinked_objects = remaining_objects;
        if linked_counter == 0 {
            break;
        }
    }

    Ok(linked)
}
