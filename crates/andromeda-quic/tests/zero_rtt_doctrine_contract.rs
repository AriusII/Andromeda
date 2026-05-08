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

use andromeda_quic::{
    EarlyDataPolicy, ZeroRttAdmissionDecision, ZeroRttAdmissionPolicy,
    ZeroRttAdmissionRejectionReason, ZeroRttReplayClass,
};

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

fn quic_src_dir() -> PathBuf {
    project_root()
        .join("crates")
        .join("andromeda-quic")
        .join("src")
}

fn runtime_src_dir() -> PathBuf {
    project_root()
        .join("crates")
        .join("andromeda-quic-runtime-quinn")
        .join("src")
}

fn owned_runtime_sources() -> Vec<PathBuf> {
    let quic_src = quic_src_dir();
    let runtime_src = runtime_src_dir();
    let mut files = vec![
        quic_src.join("catalog_manifest_resolution.rs"),
        quic_src.join("mtls_identity.rs"),
        quic_src.join("zero_rtt.rs"),
        runtime_src.join("quinn_backend.rs"),
        runtime_src.join("quinn_tls.rs"),
        runtime_src.join("runtime_quinn.rs"),
    ];
    files.extend(rust_sources_under(&quic_src.join("reconnect")));
    files.extend(rust_sources_under(&runtime_src.join("quinn_backend")));
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
fn zero_rtt_policy_rejects_mutating_unknown_and_replay_safe_classes() {
    let policy = ZeroRttAdmissionPolicy::doctrine_v1_disabled();

    let cases = [
        (
            ZeroRttReplayClass::MutatingProcedure,
            ZeroRttAdmissionRejectionReason::MutatingProcedure,
        ),
        (
            ZeroRttReplayClass::UnknownIdempotency,
            ZeroRttAdmissionRejectionReason::UnknownIdempotency,
        ),
        (
            ZeroRttReplayClass::ReadOnlyManifest,
            ZeroRttAdmissionRejectionReason::DoctrineV1DisablesEarlyData,
        ),
    ];

    for (class, reason) in cases {
        assert_eq!(
            policy.evaluate(class),
            ZeroRttAdmissionDecision::Reject { class, reason },
            "0-RTT admission must reject classified request {:?}",
            class
        );
    }
}

#[test]
fn zero_rtt_replay_safe_classes_are_classified_but_still_barred_in_v1() {
    assert!(ZeroRttReplayClass::ReadOnlyManifest.is_replay_safe());
    assert!(ZeroRttReplayClass::ReadOnlyTelemetry.is_replay_safe());
    assert!(!ZeroRttReplayClass::MutatingProcedure.is_replay_safe());

    let policy = ZeroRttAdmissionPolicy::from_early_data_policy(EarlyDataPolicy::Disabled);
    for class in [
        ZeroRttReplayClass::ReadOnlyManifest,
        ZeroRttReplayClass::ReadOnlyTelemetry,
    ] {
        assert!(!policy.evaluate(class).is_admitted());
    }
}

#[test]
fn rustls_runtime_flags_are_hard_disabled_in_source() {
    let runtime_quinn = runtime_src_dir().join("runtime_quinn.rs");
    let source = fs::read_to_string(runtime_quinn).expect("runtime_quinn.rs should exist");
    assert!(source.contains("config.max_early_data_size = 0;"));
    assert!(source.contains("config.send_half_rtt_data = false;"));
    assert!(source.contains("config.enable_early_data = false;"));
}

#[test]
fn doctrine_scan_rejects_unapproved_zero_rtt_enablement() {
    let mut violations = Vec::new();

    for path in rust_sources_under(&quic_src_dir())
        .into_iter()
        .chain(rust_sources_under(&runtime_src_dir()))
    {
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
    let mut violations = Vec::new();
    let forbidden = ["grpc", "tonic", "prost_grpc"];

    for path in rust_sources_under(&quic_src_dir())
        .into_iter()
        .chain(rust_sources_under(&runtime_src_dir()))
    {
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
fn rust_protocol_surface_does_not_introduce_raw_sql_surface() {
    let mut violations = Vec::new();
    let forbidden = ["sql", "raw_sql", "execute_sql"];

    for path in rust_sources_under(&quic_src_dir())
        .into_iter()
        .chain(rust_sources_under(&runtime_src_dir()))
    {
        let content = fs::read_to_string(&path).expect("source file must be readable");
        for (index, line) in production_source(&content).lines().enumerate() {
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
        "Rust protocol surface introduced a raw SQL application route:\n{}",
        violations.join("\n")
    );
}

#[test]
fn rust_protocol_surface_does_not_introduce_runtime_json_default() {
    let manifest = project_root()
        .join("crates")
        .join("andromeda-quic")
        .join("Cargo.toml");
    let mut violations = Vec::new();

    for path in rust_sources_under(&quic_src_dir())
        .into_iter()
        .chain(rust_sources_under(&runtime_src_dir()))
        .chain(std::iter::once(manifest))
    {
        let content = fs::read_to_string(&path).expect("protocol surface file must be readable");
        for (index, line) in production_source(&content).lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            if contains_forbidden_protocol_word(&lower, "json")
                || lower.contains("serde_json")
                || lower.contains("jsonrpc")
                || lower.contains("json-rpc")
            {
                violations.push(format!("{}:{}", path.display(), index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Rust protocol surface introduced a runtime JSON protocol default:\n{}",
        violations.join("\n")
    );
}

#[test]
fn owned_runtime_sources_do_not_use_panic_style_escape_hatches() {
    let mut violations = Vec::new();

    for path in owned_runtime_sources() {
        let content = fs::read_to_string(&path).expect("source file must be readable");
        for (index, line) in production_source(&content).lines().enumerate() {
            if line.contains(".unwrap(")
                || line.contains(".expect(")
                || line.contains("panic!(")
                || line.contains("unreachable!(")
                || line.contains("todo!(")
                || line.contains("unimplemented!(")
            {
                violations.push(format!("{}:{}", path.display(), index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Runtime QUIC source uses panic-style escape hatches:\n{}",
        violations.join("\n")
    );
}
