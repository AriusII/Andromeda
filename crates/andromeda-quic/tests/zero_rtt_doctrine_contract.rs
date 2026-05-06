//! 0-RTT doctrine lock for V1.
//!
//! This test suite enforces:
//! - `EarlyDataPolicy` remains `Disabled`-only at compile time;
//! - runtime wiring keeps rustls early data disabled; and
//! - no enabling code lands without an explicit DEC marker.

use std::{
    fs,
    path::{Path, PathBuf},
};

use andromeda_quic::EarlyDataPolicy;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate dir has workspace parent")
        .parent()
        .expect("workspace parent exists")
        .to_path_buf()
}

fn collect_rust_sources(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("source directory should be readable") {
        let entry = entry.expect("source directory entry should be readable");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_sources(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn rust_sources_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_sources(dir, &mut files);
    files.sort();
    files
}

fn owned_runtime_sources(src_dir: &Path) -> Vec<PathBuf> {
    let mut files = vec![
        src_dir.join("catalog_manifest_resolution.rs"),
        src_dir.join("mtls_identity.rs"),
        src_dir.join("quinn_backend.rs"),
        src_dir.join("runtime_quinn.rs"),
        src_dir.join("zero_rtt.rs"),
    ];
    files.extend(rust_sources_under(&src_dir.join("reconnect")));
    files.sort();
    files
}

fn contains_forbidden_protocol_word(line: &str, word: &str) -> bool {
    line.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .any(|token| token.eq_ignore_ascii_case(word))
}

fn production_source(content: &str) -> &str {
    content.split("#[cfg(test)]").next().unwrap_or(content)
}

#[test]
fn early_data_policy_is_disabled_only_variant() {
    fn compile_lock(policy: EarlyDataPolicy) -> bool {
        match policy {
            EarlyDataPolicy::Disabled => true,
        }
    }
    assert!(compile_lock(EarlyDataPolicy::Disabled));
}

#[test]
fn rustls_runtime_flags_are_hard_disabled_in_source() {
    let runtime_quinn = project_root()
        .join("crates")
        .join("andromeda-quic")
        .join("src")
        .join("runtime_quinn.rs");
    let source = fs::read_to_string(runtime_quinn).expect("runtime_quinn.rs should exist");
    assert!(source.contains("config.max_early_data_size = 0;"));
    assert!(source.contains("config.send_half_rtt_data = false;"));
    assert!(source.contains("config.enable_early_data = false;"));
}

#[test]
fn doctrine_scan_rejects_unapproved_zero_rtt_enablement() {
    let src_dir = project_root()
        .join("crates")
        .join("andromeda-quic")
        .join("src");
    let mut violations = Vec::new();

    for path in rust_sources_under(&src_dir) {
        let content = fs::read_to_string(&path).expect("source file must be readable");
        for (index, line) in content.lines().enumerate() {
            let is_enablement = line.contains("enable_early_data = true")
                || line.contains("send_half_rtt_data = true")
                || (line.contains("max_early_data_size =") && !line.contains("= 0"));
            let has_dec_marker = line.contains("DEC-");
            if is_enablement && !has_dec_marker {
                violations.push(format!("{}:{}:{}", path.display(), index + 1, line.trim()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Detected unapproved 0-RTT enablement:\n{}",
        violations.join("\n")
    );
}

#[test]
fn rust_protocol_surface_does_not_introduce_grpc_or_tonic() {
    let src_dir = project_root()
        .join("crates")
        .join("andromeda-quic")
        .join("src");
    let mut violations = Vec::new();
    let forbidden = ["grpc", "tonic", "prost_grpc"];

    for path in rust_sources_under(&src_dir) {
        let content = fs::read_to_string(&path).expect("source file must be readable");
        for (index, line) in content.lines().enumerate() {
            if forbidden
                .iter()
                .any(|word| contains_forbidden_protocol_word(line, word))
            {
                violations.push(format!("{}:{}", path.display(), index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Rust protocol surface introduced a forbidden RPC stack term:\n{}",
        violations.join("\n")
    );
}

#[test]
fn owned_runtime_sources_do_not_use_panic_unwraps() {
    let src_dir = project_root()
        .join("crates")
        .join("andromeda-quic")
        .join("src");
    let mut violations = Vec::new();

    for path in owned_runtime_sources(&src_dir) {
        let content = fs::read_to_string(&path).expect("source file must be readable");
        for (index, line) in production_source(&content).lines().enumerate() {
            if line.contains(".unwrap(") || line.contains(".expect(") {
                violations.push(format!("{}:{}", path.display(), index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Runtime QUIC source uses panic-style unwrap/expect:\n{}",
        violations.join("\n")
    );
}
