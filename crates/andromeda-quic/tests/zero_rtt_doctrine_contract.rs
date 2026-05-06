//! 0-RTT doctrine lock for V1.
//!
//! This test suite enforces:
//! - `EarlyDataPolicy` remains `Disabled`-only at compile time;
//! - runtime wiring keeps rustls early data disabled; and
//! - no enabling code lands without an explicit DEC marker.

use std::{fs, path::PathBuf};

use andromeda_quic::EarlyDataPolicy;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate dir has workspace parent")
        .parent()
        .expect("workspace parent exists")
        .to_path_buf()
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

    for entry in fs::read_dir(src_dir).expect("source directory should exist") {
        let entry = entry.expect("directory entry should be readable");
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let content = fs::read_to_string(entry.path()).expect("source file must be readable");
        for (index, line) in content.lines().enumerate() {
            let is_enablement = line.contains("enable_early_data = true")
                || line.contains("send_half_rtt_data = true")
                || (line.contains("max_early_data_size =") && !line.contains("= 0"));
            let has_dec_marker = line.contains("DEC-");
            if is_enablement && !has_dec_marker {
                violations.push(format!(
                    "{}:{}:{}",
                    entry.path().display(),
                    index + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Detected unapproved 0-RTT enablement:\n{}",
        violations.join("\n")
    );
}
