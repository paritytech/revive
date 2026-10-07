//! Reading a compilation's artifacts back into a [`Chain`].

use std::path::Path;
use std::path::PathBuf;

use revive_common::EXTENSION_LLVM_SOURCE;
use revive_common::EXTENSION_NEWYORK_MANIFEST;
use revive_newyork::StageManifest;
use revive_newyork::StageRecord;
use revive_newyork::PASS_FROM_YUL;
use revive_newyork::STAGE_MANIFEST_SCHEMA;
use revive_yul::parser::statement::object;

use crate::common::Chain;
use crate::common::Format;
use crate::common::Stage;
use crate::common::CODEGEN_STEP;
use crate::common::LLVM_DUMP_SUFFIX;

/// Returns the manifest paths in `directory`, in lexical order.
///
/// One compilation writes a manifest per contract into the debug output
/// directory, so there is no single manifest to look for by name.
pub fn manifest_paths(directory: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let entries = std::fs::read_dir(directory).map_err(|error| {
        anyhow::anyhow!(
            "reading the stage directory {}: {error}",
            directory.display()
        )
    })?;

    let suffix = format!(".{EXTENSION_NEWYORK_MANIFEST}");
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            anyhow::anyhow!(
                "reading an entry of the stage directory {}: {error}",
                directory.display()
            )
        })?;
        let path = entry.path();
        let is_manifest = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(suffix.as_str()));
        if is_manifest {
            paths.push(path);
        }
    }
    paths.sort();

    Ok(paths)
}

/// Parses everything belonging to the contract indexed by `manifest_path`.
pub fn parse_chain(manifest_path: PathBuf) -> anyhow::Result<Chain> {
    let manifest = parse_manifest(&manifest_path)?;
    let directory = manifest_path.parent().unwrap_or(Path::new(".")).to_owned();
    let stages = parse_stages(&directory, &manifest)?;
    let yul_ast = parse_yul_ast(&directory, &manifest)?;
    let dump = read_llvm_ir(&manifest_path)?;
    let (llvm_ir_name, llvm_ir) = match dump {
        Some((name, contents)) => (Some(name), Some(contents)),
        None => (None, None),
    };

    Ok(Chain {
        manifest_path,
        manifest,
        stages,
        yul_ast,
        llvm_ir,
        llvm_ir_name,
    })
}

/// Parses the manifest at `path`, rejecting a schema version this build cannot read.
fn parse_manifest(path: &Path) -> anyhow::Result<StageManifest<'static>> {
    let json = std::fs::read_to_string(path)
        .map_err(|error| anyhow::anyhow!("reading the manifest {}: {error}", path.display()))?;

    let schema = schema_version(&json, path)?;
    anyhow::ensure!(
        schema == STAGE_MANIFEST_SCHEMA,
        "the manifest {} declares schema version {schema}, which this build cannot read; \
         it understands version {STAGE_MANIFEST_SCHEMA}",
        path.display(),
    );

    let manifest: StageManifest<'_> = serde_json::from_str(&json)
        .map_err(|error| anyhow::anyhow!("parsing the manifest {}: {error}", path.display()))?;

    Ok(manifest.into_owned())
}

/// Reads just the schema version, without committing to the rest of the shape.
///
/// A manifest written by a newer compiler may have moved the fields this build
/// expects. Probing the version first reports that as the version mismatch it
/// is, rather than as a parse failure somewhere in the middle of the document.
fn schema_version(json: &str, path: &Path) -> anyhow::Result<u32> {
    let document: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| anyhow::anyhow!("parsing the manifest {}: {error}", path.display()))?;

    document
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .and_then(|schema| u32::try_from(schema).ok())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "the manifest {} has no readable `schema` version; it may not be a stage manifest",
                path.display()
            )
        })
}

/// Parses the stage artifacts `manifest` indexes, in pipeline order.
///
/// `directory` is the one the manifest was found in; it names its stage files
/// relative to that. The manifest's order is the order the passes ran in, which
/// is what [`Stage::index`] records.
fn parse_stages(directory: &Path, manifest: &StageManifest<'_>) -> anyhow::Result<Vec<Stage>> {
    let mut stages = Vec::with_capacity(manifest.stages.len());

    for (index, entry) in manifest.stages.iter().enumerate() {
        let path = directory.join(entry.file.as_str());
        let json = std::fs::read_to_string(&path).map_err(|error| {
            anyhow::anyhow!("reading the stage artifact {}: {error}", path.display())
        })?;
        let record: StageRecord<'_> = serde_json::from_str(&json).map_err(|error| {
            anyhow::anyhow!("parsing the stage artifact {}: {error}", path.display())
        })?;

        // A manifest and the files it points at can fall out of step if a
        // directory holds the output of two different compilations. Reading a
        // chain that is not the one the manifest describes would produce
        // obligations about a compilation that never happened.
        anyhow::ensure!(
            record.stage_id == entry.id,
            "the stage artifact {} holds stage `{}`, but the manifest indexes it as `{}`",
            path.display(),
            record.stage_id,
            entry.id,
        );

        stages.push(Stage {
            index,
            from: input_format(entry.pass.as_str()),
            to: Format::NewYork,
            content: record.into_owned(),
        });
    }

    Ok(stages)
}

/// The format the pass named `pass` consumed.
///
/// Only the Yul translation reads anything other than newyork IR; every
/// optimization pass rewrites the IR in place.
fn input_format(pass: &str) -> Format {
    if pass == PASS_FROM_YUL {
        Format::Yul
    } else {
        Format::NewYork
    }
}

/// Parses the Yul AST the chain starts from, when the manifest names one.
///
/// The manifest's `yul_ast` holds the *file name* of the dumped AST beside it,
/// not the AST itself, and is absent when the AST was not written.
fn parse_yul_ast(
    directory: &Path,
    manifest: &StageManifest<'_>,
) -> anyhow::Result<Option<object::Object>> {
    let Some(file) = manifest.yul_ast.as_ref() else {
        return Ok(None);
    };

    let path = directory.join(file);
    let json = std::fs::read_to_string(&path)
        .map_err(|error| anyhow::anyhow!("reading the Yul AST {}: {error}", path.display()))?;
    let yul_ast = serde_json::from_str(&json)
        .map_err(|error| anyhow::anyhow!("parsing the Yul AST {}: {error}", path.display()))?;

    Ok(Some(yul_ast))
}

/// Reads the LLVM IR codegen emitted, when the dump is beside the manifest.
///
/// The codegen dumps are not listed in the manifest; they share its file-name
/// stem, which is what locates them. They are written whenever an output
/// directory is configured, which `--emit-ir-stages` requires, so the dump is
/// normally there.
fn read_llvm_ir(manifest_path: &Path) -> anyhow::Result<Option<(String, String)>> {
    let Some(name) = manifest_path.file_name().and_then(|name| name.to_str()) else {
        return Ok(None);
    };
    let Some(stem) = name.strip_suffix(&format!(".{EXTENSION_NEWYORK_MANIFEST}")) else {
        return Ok(None);
    };

    let path =
        manifest_path.with_file_name(format!("{stem}.{LLVM_DUMP_SUFFIX}.{EXTENSION_LLVM_SOURCE}"));
    if !path.is_file() {
        return Ok(None);
    }

    let llvm_ir = std::fs::read_to_string(&path)
        .map_err(|error| anyhow::anyhow!("reading the LLVM IR {}: {error}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(CODEGEN_STEP)
        .to_owned();

    Ok(Some((name, llvm_ir)))
}
