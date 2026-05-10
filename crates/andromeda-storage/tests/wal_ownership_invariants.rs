#![forbid(unsafe_code)]

//! Regression guard for WAL/storage canonical ownership.
//!
//! Each load-bearing WAL struct/enum is defined in exactly one source file.
//! This test walks every `.rs` file under the WAL and storage crates, counts
//! top-level `pub struct` / `pub enum` declarations for the WAL ownership
//! surface, and asserts:
//!
//! 1. Each type is defined exactly once across the crate.
//! 2. The single definition lives at the documented canonical path.
//!
//! Storage must not grow compatibility files for WAL/recovery owner types.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

/// Map of WAL ownership types to their canonical, workspace-relative source
/// path (forward-slash form). Update this table only when canonical ownership
/// intentionally moves; do not duplicate definitions to silence the test.
const CANONICAL_OWNERSHIP: &[(&str, &str, &str)] = &[
    // (kind, name, canonical workspace-relative path)
    ("struct", "Lsn", "crates/andromeda-wal/src/lsn.rs"),
    (
        "enum",
        "WalRecordKind",
        "crates/andromeda-wal/src/write_ahead_log/record.rs",
    ),
    (
        "struct",
        "WalRecord",
        "crates/andromeda-wal/src/write_ahead_log/record.rs",
    ),
    (
        "struct",
        "WalRecordHeader",
        "crates/andromeda-wal/src/write_ahead_log/record.rs",
    ),
    (
        "struct",
        "WalSegment",
        "crates/andromeda-wal/src/wal_segment.rs",
    ),
    (
        "struct",
        "WalSegmentDescriptor",
        "crates/andromeda-wal/src/wal_segment.rs",
    ),
    (
        "struct",
        "InMemoryWal",
        "crates/andromeda-wal/src/write_ahead_log/manager.rs",
    ),
    (
        "struct",
        "FileWal",
        "crates/andromeda-wal/src/file_wal/wal.rs",
    ),
    (
        "struct",
        "FileWalHeader",
        "crates/andromeda-wal/src/file_wal/header.rs",
    ),
    (
        "struct",
        "FileWalDiskScan",
        "crates/andromeda-wal/src/file_wal/scan.rs",
    ),
];

#[test]
fn wal_ownership_types_have_single_canonical_definition() {
    let storage_src = workspace_root().join("crates/andromeda-storage/src");
    let wal_src = workspace_root().join("crates/andromeda-wal/src");
    let workspace = workspace_root();
    let mut occurrences: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();

    for src_dir in [&storage_src, &wal_src] {
        for source in collect_rs_files(src_dir) {
            let text = fs::read_to_string(&source).expect("read WAL/storage source file");
            let stripped = strip_comments(&text);
            for (kind, name) in extract_top_level_pub_type_decls(&stripped) {
                let key = (kind, name);
                occurrences
                    .entry(key)
                    .or_default()
                    .push(workspace_relative_path(&workspace, &source));
            }
        }
    }

    let mut failures: Vec<String> = Vec::new();

    for (kind, name, expected_path) in CANONICAL_OWNERSHIP {
        let key = ((*kind).to_string(), (*name).to_string());
        match occurrences.get(&key) {
            None => failures.push(format!(
                "missing canonical definition for `pub {kind} {name}` (expected at {expected_path})"
            )),
            Some(paths) if paths.len() != 1 => failures.push(format!(
                "duplicate definitions for `pub {kind} {name}`: {paths:?} (only {expected_path} is canonical)"
            )),
            Some(paths) => {
                let actual = &paths[0];
                if actual != expected_path {
                    failures.push(format!(
                        "`pub {kind} {name}` is defined at {actual} but the canonical path is {expected_path}"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "WAL canonical ownership violations detected:\n  - {}",
        failures.join("\n  - ")
    );
}

#[test]
fn pure_wal_facade_files_stay_demolished() {
    let workspace = workspace_root();
    let demolished_facades = [
        "crates/andromeda-storage/src/recovery.rs",
        "crates/andromeda-storage/src/write_ahead_log/mod.rs",
        "crates/andromeda-storage/src/write_ahead_log/codec.rs",
        "crates/andromeda-storage/src/write_ahead_log/commit_log_entry.rs",
        "crates/andromeda-storage/src/write_ahead_log/commit_log_facade.rs",
        "crates/andromeda-storage/src/write_ahead_log/compaction.rs",
        "crates/andromeda-storage/src/write_ahead_log/durability_fence.rs",
        "crates/andromeda-storage/src/write_ahead_log/gc.rs",
        "crates/andromeda-storage/src/write_ahead_log/gc_eligibility.rs",
        "crates/andromeda-storage/src/write_ahead_log/heap_redo.rs",
        "crates/andromeda-storage/src/write_ahead_log/manager.rs",
        "crates/andromeda-storage/src/write_ahead_log/record.rs",
        "crates/andromeda-storage/src/write_ahead_log/record_bounds.rs",
        "crates/andromeda-storage/src/write_ahead_log/segment.rs",
        "crates/andromeda-storage/src/write_ahead_log/segment_reclaimability.rs",
        "crates/andromeda-storage/src/write_ahead_log/shipping.rs",
        "crates/andromeda-storage/src/write_ahead_log/shipping/tests.rs",
        "crates/andromeda-storage/src/write_ahead_log/transaction.rs",
    ];

    let mut failures: Vec<String> = Vec::new();
    for relative in demolished_facades {
        if workspace.join(relative).exists() {
            failures.push(format!(
                "{relative} reintroduced a storage WAL/recovery facade file; import from andromeda-wal, andromeda-recovery, andromeda-manifest, or andromeda-storage-page"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "WAL facade demolition regressions:\n  - {}",
        failures.join("\n  - ")
    );
}

#[test]
fn extractor_finds_pub_struct_and_enum_declarations() {
    let source = "pub struct A { f: u8 }\n\
                  pub enum B { X }\n\
                  struct Hidden;\n\
                  pub(crate) struct Crate;\n\
                  fn pub_trick() { struct Inner; }\n";
    let mut decls = extract_top_level_pub_type_decls(source);
    decls.sort();
    assert_eq!(
        decls,
        vec![
            ("enum".to_string(), "B".to_string()),
            ("struct".to_string(), "A".to_string()),
        ]
    );
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root resolves")
}

fn collect_rs_files(src_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk(src_dir, &mut |path| {
        if path.extension() == Some(OsStr::new("rs")) {
            files.push(path.to_path_buf());
        }
    });
    files
}

fn walk(dir: &Path, visit: &mut dyn FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, visit);
        } else {
            visit(&path);
        }
    }
}

/// Extract top-level `pub struct`/`pub enum` declarations. Only items declared
/// at column zero (no leading whitespace) and with bare `pub` visibility are
/// counted, which deliberately excludes inner items, restricted-visibility
/// declarations like `pub(crate)`, and tests inside `#[cfg(test)] mod tests`.
fn extract_top_level_pub_type_decls(source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in source.lines() {
        // Require column-zero `pub ` to skip nested items.
        let Some(rest) = line.strip_prefix("pub ") else {
            continue;
        };
        let rest = rest.trim_start();
        let kind = if let Some(after) = rest.strip_prefix("struct ") {
            ("struct", after)
        } else if let Some(after) = rest.strip_prefix("enum ") {
            ("enum", after)
        } else {
            continue;
        };
        let after = kind.1.trim_start();
        let ident_end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        if ident_end == 0 {
            continue;
        }
        let name = &after[..ident_end];
        out.push((kind.0.to_string(), name.to_string()));
    }
    out
}

fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut in_block = false;
    while index < bytes.len() {
        if in_block {
            if index + 1 < bytes.len() && bytes[index] == b'*' && bytes[index + 1] == b'/' {
                in_block = false;
                index += 2;
            } else {
                if bytes[index] == b'\n' {
                    out.push('\n');
                }
                index += 1;
            }
            continue;
        }
        if index + 1 < bytes.len() && bytes[index] == b'/' && bytes[index + 1] == b'/' {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if index + 1 < bytes.len() && bytes[index] == b'/' && bytes[index + 1] == b'*' {
            in_block = true;
            index += 2;
            continue;
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
}

fn workspace_relative_path(workspace: &Path, path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let workspace = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    canonical
        .strip_prefix(&workspace)
        .unwrap_or(&canonical)
        .to_string_lossy()
        .replace('\\', "/")
}
