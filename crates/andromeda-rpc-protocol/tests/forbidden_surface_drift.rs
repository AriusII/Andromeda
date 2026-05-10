use std::{fs, path::PathBuf};

const CARGO_MANIFEST: &str = include_str!("../Cargo.toml");

#[test]
fn cargo_manifest_rejects_rpc_runtime_and_implicit_wire_stack_drift() {
    let manifest = CARGO_MANIFEST.to_ascii_lowercase();
    let forbidden = [
        "andromeda-quic",
        "andromeda-quic-runtime-quinn",
        "andromeda-rpc-runtime",
        "andromeda-runtime-quinn",
        "bincode",
        "bytemuck",
        "grpc",
        "grpcio",
        "h2",
        "hyper",
        "json-rpc",
        "jsonrpc",
        "jsonrpsee",
        "postcard",
        "prost",
        "prost-grpc",
        "prost_grpc",
        "quinn",
        "rcgen",
        "rkyv",
        "rustls",
        "serde_json",
        "tokio",
        "tokio-rustls",
        "tonic",
        "tower",
        "zerocopy",
    ];

    let violations = forbidden
        .into_iter()
        .filter(|term| manifest.contains(term))
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "andromeda-rpc-protocol manifest introduced forbidden runtime or implicit wire dependencies: {:?}",
        violations
    );
}

#[test]
fn source_rejects_runtime_transport_imports() {
    let forbidden = [
        "andromeda_quic::",
        "andromeda_quic_runtime_quinn::",
        "andromeda_rpc_runtime::",
        "andromeda_runtime_quinn::",
        "bincode::",
        "prost::",
        "quinn::",
        "rcgen::",
        "rkyv::",
        "rustls::",
        "serde_json::",
        "tokio::",
        "tonic::",
        "zerocopy::",
    ];
    let violations = rust_sources()
        .into_iter()
        .flat_map(|path| source_violations(path, &forbidden))
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "andromeda-rpc-protocol source introduced forbidden runtime transport imports:\n{}",
        violations.join("\n")
    );
}

#[test]
fn source_rejects_native_layout_or_implicit_codec_wire_paths() {
    let forbidden = [
        "#[repr(C)]",
        "#[repr(packed)]",
        "core::mem::transmute",
        "std::mem::transmute",
        "from_ne_bytes",
        "from_raw_parts",
        "to_ne_bytes",
    ];
    let violations = rust_sources()
        .into_iter()
        .flat_map(|path| source_violations(path, &forbidden))
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "andromeda-rpc-protocol source introduced forbidden native-layout or implicit codec wire paths:\n{}",
        violations.join("\n")
    );
}

#[test]
fn public_surface_remains_protocol_only_without_runtime_modules() {
    let lib = fs::read_to_string(crate_root().join("src").join("lib.rs"))
        .expect("andromeda-rpc-protocol lib.rs must be readable");
    let forbidden = [
        "pub mod runtime",
        "pub mod server",
        "pub mod transport",
        "pub use quinn",
        "pub use tokio",
        "pub use tonic",
    ];

    let violations = forbidden
        .into_iter()
        .filter(|term| lib.contains(term))
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "andromeda-rpc-protocol public surface drifted into runtime ownership: {:?}",
        violations
    );
}

#[test]
fn source_rejects_surface_authorization_or_admin_policy_ownership() {
    let forbidden = [
        "AdminOperation",
        "ClusterPromote",
        "HighAvailability",
        "ManageSecurity",
        "SurfaceAction",
        "SurfacePlane",
        "SurfaceScope",
        "requires_application_surface",
    ];
    let violations = rust_sources()
        .into_iter()
        .flat_map(|path| source_violations(path, &forbidden))
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "andromeda-rpc-protocol source introduced surface authorization or admin policy ownership:\n{}",
        violations.join("\n")
    );
}

fn rust_sources() -> Vec<PathBuf> {
    fs::read_dir(crate_root().join("src"))
        .expect("andromeda-rpc-protocol src directory must be readable")
        .map(|entry| {
            entry
                .expect("source directory entry must be readable")
                .path()
        })
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect()
}

fn source_violations(path: PathBuf, forbidden: &[&str]) -> Vec<String> {
    let source = fs::read_to_string(&path).expect("Rust source file must be readable");
    forbidden
        .iter()
        .filter(|term| source.contains(**term))
        .map(|term| format!("{} contains {}", path.display(), term))
        .collect()
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
