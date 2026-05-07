#![forbid(unsafe_code)]

//! Regression guard for DEC-032 page/LSN canonical ownership.
//!
//! Future buffer-pool, heap, and index work must import the existing durable
//! primitives from the storage crate root or the `layout::page` compatibility
//! facade. They must not introduce mirror `PageId`, `PageSize`, `PageHeader`,
//! `PageTrailer`, `PageLayoutContract`, or `Lsn` definitions.

use std::any::TypeId;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use andromeda_storage as storage;
use andromeda_storage::layout;

/// Map of page/LSN ownership types to their canonical, workspace-relative
/// source path (forward-slash form). Update this table only when canonical
/// ownership intentionally moves; do not duplicate definitions to silence the
/// test.
const CANONICAL_OWNERSHIP: &[(&str, &str, &str)] = &[
    (
        "struct",
        "PageId",
        "crates/andromeda-storage/src/page/identity.rs",
    ),
    (
        "enum",
        "PageSize",
        "crates/andromeda-storage/src/page/layout.rs",
    ),
    (
        "struct",
        "PageHeader",
        "crates/andromeda-storage/src/page/layout.rs",
    ),
    (
        "struct",
        "PageTrailer",
        "crates/andromeda-storage/src/page/layout.rs",
    ),
    (
        "struct",
        "PageLayoutContract",
        "crates/andromeda-storage/src/page/layout.rs",
    ),
    ("struct", "Lsn", "crates/andromeda-storage/src/lsn.rs"),
];

#[test]
fn page_and_lsn_types_have_single_canonical_definition() {
    let storage_src = workspace_root().join("crates/andromeda-storage/src");
    let workspace = workspace_root();
    let mut occurrences: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();

    for source in collect_rs_files(&storage_src) {
        let text = fs::read_to_string(&source).expect("read storage source file");
        let stripped = strip_comments(&text);
        for (kind, name) in extract_top_level_pub_type_decls(&stripped) {
            occurrences
                .entry((kind, name))
                .or_default()
                .push(workspace_relative_path(&workspace, &source));
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
        "DEC-032 page/LSN canonical ownership violations detected:\n  - {}",
        failures.join("\n  - ")
    );
}

#[test]
fn buffer_heap_and_index_style_imports_resolve_to_existing_primitives() {
    fn assert_type<T: 'static>() -> TypeId {
        TypeId::of::<T>()
    }

    fn accept_page_id(_id: storage::PageId) {}
    fn accept_page_size(_size: storage::PageSize) {}
    fn accept_header(_header: storage::PageHeader) {}
    fn accept_trailer(_trailer: storage::PageTrailer) {}
    fn accept_contract(_contract: storage::PageLayoutContract) {}
    fn accept_lsn(_lsn: storage::Lsn) {}

    assert_eq!(
        assert_type::<storage::PageId>(),
        assert_type::<layout::page::PageId>()
    );
    assert_eq!(
        assert_type::<storage::PageSize>(),
        assert_type::<layout::page::PageSize>()
    );
    assert_eq!(
        assert_type::<storage::PageHeader>(),
        assert_type::<layout::page::PageHeader>()
    );
    assert_eq!(
        assert_type::<storage::PageTrailer>(),
        assert_type::<layout::page::PageTrailer>()
    );
    assert_eq!(
        assert_type::<storage::PageLayoutContract>(),
        assert_type::<layout::page::PageLayoutContract>()
    );

    let header = storage::PageHeader {
        magic: storage::PageHeader::MAGIC,
        format_version: storage::PageHeader::FORMAT_VERSION_V0,
        page_size: layout::page::PageSize::KiB16,
        page_type: layout::page::PageType::FixedRow,
        page_id: layout::page::PageId::new(1),
        object_id: layout::page::ObjectId::new(2),
        allocation_id: layout::page::AllocationId::new(3),
        page_lsn: storage::Lsn::new(4),
        page_epoch: 1,
        previous_page_id: None,
        next_page_id: None,
        header_len: storage::PageHeader::MIN_HEADER_LEN_V0,
        payload_offset: 128,
        payload_len: 512,
        free_start: 256,
        free_end: 512,
        free_bytes: 256,
        slot_count: 1,
        row_count: 1,
        flags: layout::page::PageFlags::NONE,
        header_crc: 5,
    };
    let trailer = layout::page::PageTrailer {
        payload_crc64: 6,
        page_hash: [7; 32],
        torn_write_guard: 8,
    };
    let contract = layout::page::PageLayoutContract { header, trailer };
    assert!(contract.validate().is_ok());

    accept_page_id(layout::page::PageId::new(9));
    accept_page_size(layout::page::PageSize::KiB16);
    accept_header(contract.header);
    accept_trailer(contract.trailer);
    accept_contract(contract);
    accept_lsn(storage::Lsn::new(10));
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
/// counted, which deliberately excludes inner items and restricted-visibility
/// declarations like `pub(crate)`.
fn extract_top_level_pub_type_decls(source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in source.lines() {
        let Some(rest) = line.strip_prefix("pub ") else {
            continue;
        };
        let rest = rest.trim_start();
        let (kind, after) = if let Some(after) = rest.strip_prefix("struct ") {
            ("struct", after)
        } else if let Some(after) = rest.strip_prefix("enum ") {
            ("enum", after)
        } else {
            continue;
        };
        let after = after.trim_start();
        let ident_end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        if ident_end == 0 {
            continue;
        }
        out.push((kind.to_string(), after[..ident_end].to_string()));
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
