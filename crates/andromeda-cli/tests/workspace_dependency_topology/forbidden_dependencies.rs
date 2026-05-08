use crate::{
    diagnostics::{
        exec_runtime_quinn_feature_violations, relative_slash_path, rust_source_files,
        strip_rust_comments,
    },
    manifest_loading::load_crate_manifests,
    workspace_root,
};
use std::fs;
const CONCRETE_RUNTIME_TLS_QUIC_DEPS: [&str; 3] = ["quinn", "rcgen", "rustls"];
const EXEC_FORBIDDEN_DIRECT_RUNTIME_DEPS: &[&str] = &[
    "andromeda-runtime-quinn",
    "grpc",
    "grpc-web",
    "grpcio",
    "grpcio-sys",
    "prost-grpc",
    "quinn",
    "rcgen",
    "rustls",
    "serde-json",
    "tonic",
    "tonic-build",
    "tonic-health",
    "tonic-prost",
    "tonic-prost-build",
    "tonic-reflection",
    "tonic-transport",
    "tonic-web",
    "tower-grpc",
];
const EXEC_FORBIDDEN_SOURCE_TOKENS: &[&str] = &[
    "quinn::",
    "rcgen::",
    "rustls::",
    "tonic::",
    "grpc::",
    "grpcio::",
    "prost_grpc::",
    "tower_grpc::",
    "serde_json",
];
const RPC_PROTOCOL_FORBIDDEN_RUNTIME_DEPS: &[&str] = &[
    "grpc",
    "grpcio",
    "grpcio-sys",
    "h2",
    "hyper",
    "quinn",
    "rcgen",
    "rustls",
    "tokio",
    "tonic",
    "tonic-build",
    "tonic-prost",
    "tonic-prost-build",
    "tonic-transport",
    "tonic-web",
    "tower",
];
const RPC_PROTOCOL_FORBIDDEN_SOURCE_TOKENS: &[&str] = &[
    "quinn::",
    "rcgen::",
    "rustls::",
    "tokio::",
    "tonic::",
    "grpc::",
    "runtime_quinn",
    "quinn_backend",
    "quinn_tls",
    "#[tokio::",
];
pub(crate) const SECURITY_CONTRACT_FORBIDDEN_RUNTIME_DEPS: &[&str] = &[
    "andromeda-core",
    "andromeda-contract",
    "andromeda-catalog",
    "andromeda-proto",
    "andromeda-rpc-protocol",
    "andromeda-quic",
    "andromeda-observe",
    "andromeda-analytics",
    "andromeda-bench",
    "andromeda-gpu",
    "andromeda-gpu-kernels",
    "andromeda-rpc-runtime",
    "andromeda-runtime-quinn",
    "andromeda-exec",
    "andromeda-storage",
    "andromeda-wal",
    "andromeda-tx",
    "quinn",
    "rcgen",
    "rustls",
    "tokio",
    "tokio-rustls",
    "h2",
    "hyper",
    "tower",
    "tonic",
    "tonic-build",
    "tonic-prost",
    "tonic-prost-build",
    "tonic-transport",
    "tonic-web",
    "grpc",
    "grpcio",
    "grpcio-sys",
    "prost",
    "prost-types",
    "prost-build",
    "protoc-bin-vendored",
    "serde",
    "serde-json",
    "json",
    "json-rpc",
    "jsonrpc",
    "jsonrpc-core",
    "jsonrpsee",
    "serde-json-core",
    "simd-json",
    "sonic-rs",
    "bincode",
    "rkyv",
    "bytemuck",
    "zerocopy",
    "abomonation",
    "bitcode",
    "borsh",
    "postcard",
    "speedy",
    "sqlx",
    "rusqlite",
    "diesel",
    "mysql",
    "mysql-async",
    "postgres",
    "sea-orm",
    "sea-query",
    "tokio-postgres",
    "ash",
    "cuda",
    "cudarc",
    "cust",
    "metal",
    "naga",
    "nvml-wrapper",
    "opencl3",
    "vulkano",
    "wgpu",
];
const SECURITY_CONTRACT_FORBIDDEN_SOURCE_TOKENS: &[&str] = &[
    "andromeda_core::",
    "andromeda_contract::",
    "andromeda_catalog::",
    "andromeda_proto::",
    "andromeda_rpc_protocol::",
    "andromeda_quic::",
    "andromeda_observe::",
    "andromeda_rpc_runtime::",
    "andromeda_runtime_quinn::",
    "andromeda_exec::",
    "andromeda_storage::",
    "andromeda_wal::",
    "andromeda_tx::",
    "quinn::",
    "rcgen::",
    "rustls::",
    "tokio::",
    "tonic::",
    "grpc::",
    "runtime_quinn",
    "quinn_backend",
    "quinn_tls",
    "#[tokio::",
    "std::net::",
    "std::fs::",
    "std::process::",
    "tonic::",
    "grpc::",
    "serde_json",
    "json!",
    "jsonrpc",
    "application/json",
    "PrincipalRegistry",
    "PrincipalBindingStore",
    "RevocationStore",
    "RoleStore",
    "PolicyStore",
    "asu8",
    "asu16",
    "asu32",
    "asusize",
    "unsafe{",
    "unsafefn",
    "unsafeimpl",
    ".unwrap(",
    ".expect(",
    "panic!",
    "todo!",
    "unimplemented!",
    "unreachable!",
];
pub(crate) const SECURITY_CRITICAL_PATH_CRATES: &[&str] = &[
    "andromeda-core",
    "andromeda-observe",
    "andromeda-proto",
    "andromeda-rpc-protocol",
    "andromeda-security-contract",
];
pub(crate) const FORBIDDEN_SECURITY_CRITICAL_GPU_RUNTIME_DEPS: &[&str] = &[
    "andromeda-analytics",
    "andromeda-bench",
    "andromeda-gpu",
    "andromeda-gpu-kernels",
    "ash",
    "cuda",
    "cudarc",
    "cust",
    "metal",
    "naga",
    "nvml-wrapper",
    "opencl3",
    "vulkano",
    "wgpu",
];
const SECURITY_CRITICAL_SOURCE_ROOTS: &[&str] = &[
    "crates/andromeda-core/src/principal",
    "crates/andromeda-observe/src/events/audit",
    "crates/andromeda-observe/src/query",
    "crates/andromeda-proto/src/manifest",
    "crates/andromeda-rpc-protocol/src",
];
const SECURITY_CRITICAL_CAST_CONTEXT_TOKENS: &[&str] = &[
    "adminoperation",
    "ordinal",
    "permission",
    "policy",
    "principal",
    "role",
    "surface",
];
#[test]
fn only_andromeda_quic_declares_concrete_runtime_tls_quic_crates() {
    let manifests = load_crate_manifests(&workspace_root().join("crates"));
    let mut violations = Vec::new();

    for manifest in manifests.values() {
        if manifest.package_name == "andromeda-quic" {
            continue;
        }

        for dependency in &manifest.runtime_dependencies {
            if CONCRETE_RUNTIME_TLS_QUIC_DEPS.contains(&dependency.as_str()) {
                violations.push(format!(
                    "{} declares production dependency `{dependency}` in {}; concrete Quinn/TLS crates must stay owned by andromeda-quic until runtime extraction",
                    manifest.package_name,
                    manifest.path.display()
                ));
            }
        }

        for dependency in &manifest.dev_dependencies {
            if CONCRETE_RUNTIME_TLS_QUIC_DEPS.contains(&dependency.as_str()) {
                violations.push(format!(
                    "{} declares dev dependency `{dependency}` in {}; concrete Quinn/TLS crates must stay owned by andromeda-quic until runtime extraction",
                    manifest.package_name,
                    manifest.path.display()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "concrete runtime TLS/QUIC dependency ownership violations:\n{}",
        violations.join("\n")
    );
}
#[test]
fn andromeda_exec_runtime_quinn_boundary_stays_feature_reexport_only() {
    let workspace = workspace_root();
    let manifests = load_crate_manifests(&workspace.join("crates"));
    let manifest = manifests
        .get("andromeda-exec")
        .expect("workspace must include andromeda-exec");
    let mut violations = Vec::new();

    for dependency in manifest
        .runtime_dependencies
        .iter()
        .chain(manifest.dev_dependencies.iter())
    {
        if EXEC_FORBIDDEN_DIRECT_RUNTIME_DEPS.contains(&dependency.as_str()) {
            violations.push(format!(
                "andromeda-exec must not directly depend on `{dependency}` in {}; runtime-quinn must only reexport andromeda-quic/runtime-quinn",
                manifest.path.display()
            ));
        }
    }

    let manifest_text = fs::read_to_string(&manifest.path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest.path.display()));
    violations.extend(exec_runtime_quinn_feature_violations(&manifest_text));

    let exec_src = workspace.join("crates/andromeda-exec/src");
    for file in rust_source_files(&exec_src) {
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
        let code_without_comments = strip_rust_comments(&source);
        let relative = relative_slash_path(&workspace, &file);

        for (line_index, line) in code_without_comments.lines().enumerate() {
            let compact_line = line
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>();

            for token in EXEC_FORBIDDEN_SOURCE_TOKENS {
                if compact_line.contains(token) {
                    violations.push(format!(
                        "{relative}:{} exposes direct concrete runtime token `{token}`; andromeda-exec must route Quinn through the runtime-quinn feature reexport only",
                        line_index + 1
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "andromeda-exec runtime-quinn boundary violations:\n{}",
        violations.join("\n")
    );
}
#[test]
fn rpc_protocol_crate_stays_runtime_free_in_manifest_and_source() {
    let workspace = workspace_root();
    let manifests = load_crate_manifests(&workspace.join("crates"));
    let manifest = manifests
        .get("andromeda-rpc-protocol")
        .expect("workspace must include andromeda-rpc-protocol");
    let mut violations = Vec::new();

    for dependency in manifest
        .runtime_dependencies
        .iter()
        .chain(manifest.dev_dependencies.iter())
    {
        if RPC_PROTOCOL_FORBIDDEN_RUNTIME_DEPS.contains(&dependency.as_str()) {
            violations.push(format!(
                "andromeda-rpc-protocol manifest must not depend on runtime crate `{dependency}`"
            ));
        }
    }

    let rpc_protocol_src = workspace.join("crates/andromeda-rpc-protocol/src");
    for file in rust_source_files(&rpc_protocol_src) {
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
        let code_without_comments = strip_rust_comments(&source);
        let relative = relative_slash_path(&workspace, &file);

        for (line_index, line) in code_without_comments.lines().enumerate() {
            let compact_line = line
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>();

            for token in RPC_PROTOCOL_FORBIDDEN_SOURCE_TOKENS {
                if compact_line.contains(token) {
                    violations.push(format!(
                        "{relative}:{} exposes runtime token `{token}`",
                        line_index + 1
                    ));
                }
            }

            if compact_line.starts_with("pubmodruntime") || compact_line.starts_with("modruntime") {
                violations.push(format!(
                    "{relative}:{} declares a runtime module from the abstract protocol crate",
                    line_index + 1
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "andromeda-rpc-protocol must remain runtime-free:\n{}",
        violations.join("\n")
    );
}
#[test]
fn security_contract_crate_stays_runtime_free_in_manifest_and_source_when_present() {
    let workspace = workspace_root();
    let manifests = load_crate_manifests(&workspace.join("crates"));
    let Some(manifest) = manifests.get("andromeda-security-contract") else {
        return;
    };
    let mut violations = Vec::new();

    for dependency in manifest
        .runtime_dependencies
        .iter()
        .chain(manifest.dev_dependencies.iter())
    {
        if SECURITY_CONTRACT_FORBIDDEN_RUNTIME_DEPS.contains(&dependency.as_str()) {
            violations.push(format!(
                "andromeda-security-contract manifest must not depend on runtime crate `{dependency}`"
            ));
        }
    }

    let security_contract_src = workspace.join("crates/andromeda-security-contract/src");
    if security_contract_src.is_dir() {
        for file in rust_source_files(&security_contract_src) {
            let source = fs::read_to_string(&file)
                .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
            let code_without_comments = strip_rust_comments(&source);
            let relative = relative_slash_path(&workspace, &file);

            for (line_index, line) in code_without_comments.lines().enumerate() {
                let compact_line = line
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect::<String>();

                for token in SECURITY_CONTRACT_FORBIDDEN_SOURCE_TOKENS.iter().copied() {
                    if compact_line.contains(token) {
                        violations.push(format!(
                            "{relative}:{} exposes runtime token `{token}`",
                            line_index + 1
                        ));
                    }
                }

                if compact_line.starts_with("pubmodruntime")
                    || compact_line.starts_with("modruntime")
                {
                    violations.push(format!(
                        "{relative}:{} declares a runtime module from the security contract crate",
                        line_index + 1
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "andromeda-security-contract must remain runtime-free:\n{}",
        violations.join("\n")
    );
}
#[test]
fn security_critical_sources_do_not_use_implicit_surface_permission_ordinal_casts() {
    let workspace = workspace_root();
    let mut violations = Vec::new();

    for root in SECURITY_CRITICAL_SOURCE_ROOTS {
        let root = workspace.join(root);
        if !root.is_dir() {
            continue;
        }

        for file in rust_source_files(&root) {
            let source = fs::read_to_string(&file)
                .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
            let code_without_comments = strip_rust_comments(&source);
            let relative = relative_slash_path(&workspace, &file);

            for (line_index, line) in code_without_comments.lines().enumerate() {
                let compact_line = line
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect::<String>()
                    .to_ascii_lowercase();
                if security_critical_cast_uses_implicit_ordinal(&compact_line) {
                    violations.push(format!(
                        "{relative}:{} uses implicit ordinal cast `as u8` for security surface/permission semantics",
                        line_index + 1
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "security-critical sources must not cast surface/permission ordinals with `as u8` outside andromeda-security-contract:\n{}",
        violations.join("\n")
    );
}
fn security_critical_cast_uses_implicit_ordinal(compact_line: &str) -> bool {
    compact_line.contains("asu8")
        && SECURITY_CRITICAL_CAST_CONTEXT_TOKENS
            .iter()
            .any(|token| compact_line.contains(token))
}
