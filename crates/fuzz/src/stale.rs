//! Warn if the `resolc` on `$PATH` is older than any workspace
//! source: `Project::compile` spawns it via `--recursive-process`,
//! so a stale binary silently masks local source changes.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// One-shot resolc-staleness check (idempotent via `Once`).
pub fn warn_if_resolc_stale() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(check);
}

fn check() {
    let resolc_path = match which::which("resolc") {
        Ok(path) => path,
        Err(_) => {
            log::warn!("`resolc` not found on $PATH; `make install-bin` first.");
            return;
        }
    };
    let Ok(resolc_mtime) = resolc_path.metadata().and_then(|m| m.modified()) else {
        return;
    };

    // crates/fuzz/.. = crates/ — the source root to scan.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let Some(crates_dir) = manifest_dir.parent().map(Path::to_path_buf) else {
        return;
    };
    if !crates_dir.exists() {
        return;
    }
    let Some(newest) = newest_source_mtime(&crates_dir) else {
        return;
    };

    if newest > resolc_mtime {
        log::warn!(
            "installed `resolc` at {} is older than revive source ({} vs {}).",
            resolc_path.display(),
            format_age_since(resolc_mtime),
            format_age_since(newest),
        );
        log::warn!(
            "the fuzzer shells out to this binary — run `make install-bin` to rebuild before trusting results."
        );
    }
}

/// Newest mtime under `crates/<member>/{src/**, Cargo.toml}`, minus
/// `crates/fuzz` (editing it isn't a reason to warn).
fn newest_source_mtime(crates_dir: &Path) -> Option<SystemTime> {
    let mut newest: Option<SystemTime> = None;
    let entries = std::fs::read_dir(crates_dir).ok()?;
    for entry in entries.flatten() {
        let crate_dir = entry.path();
        if !crate_dir.is_dir() {
            continue;
        }
        if crate_dir.file_name().is_some_and(|n| n == "fuzz") {
            continue;
        }
        update_newest_recursive(&crate_dir.join("src"), &mut newest);
        if let Ok(meta) = crate_dir.join("Cargo.toml").metadata() {
            if let Ok(mtime) = meta.modified() {
                bump(&mut newest, mtime);
            }
        }
    }
    newest
}

fn update_newest_recursive(dir: &Path, newest: &mut Option<SystemTime>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            update_newest_recursive(&path, newest);
        } else if meta.is_file() {
            if let Ok(mtime) = meta.modified() {
                bump(newest, mtime);
            }
        }
    }
}

fn bump(newest: &mut Option<SystemTime>, candidate: SystemTime) {
    match newest {
        Some(current) if *current >= candidate => {}
        _ => *newest = Some(candidate),
    }
}

fn format_age_since(mtime: SystemTime) -> String {
    match SystemTime::now().duration_since(mtime) {
        Ok(duration) => {
            let secs = duration.as_secs();
            if secs < 60 {
                format!("{secs}s ago")
            } else if secs < 3600 {
                format!("{}m ago", secs / 60)
            } else if secs < 86400 {
                format!("{}h ago", secs / 3600)
            } else {
                format!("{}d ago", secs / 86400)
            }
        }
        Err(_) => "in the future".into(),
    }
}
