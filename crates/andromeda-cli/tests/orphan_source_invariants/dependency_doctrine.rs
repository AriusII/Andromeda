use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::dependency_manifest::{
    WorkspaceDependencyAliases, collect_dependency_manifests,
    crate_name_uses_generic_topology_bucket, parse_dependency_manifest,
    parse_dependency_manifest_with_aliases,
};
use crate::support::workspace_root;

const FORBIDDEN_PRODUCTION_EDGES: &[(&str, &str)] = &[
    ("andromeda-proto", "andromeda-quic"),
    ("andromeda-proto", "andromeda-rpc-runtime"),
    ("andromeda-proto", "andromeda-runtime-quinn"),
    ("andromeda-proto", "quinn"),
    ("andromeda-proto", "rcgen"),
    ("andromeda-proto", "rustls"),
    ("andromeda-proto", "tokio"),
    ("andromeda-rpc-protocol", "andromeda-quic"),
    ("andromeda-rpc-protocol", "andromeda-rpc-runtime"),
    ("andromeda-rpc-protocol", "andromeda-runtime-quinn"),
    ("andromeda-rpc-protocol", "andromeda-exec"),
    ("andromeda-rpc-protocol", "andromeda-storage"),
    ("andromeda-rpc-protocol", "andromeda-wal"),
    ("andromeda-rpc-protocol", "quinn"),
    ("andromeda-rpc-protocol", "rcgen"),
    ("andromeda-rpc-protocol", "rustls"),
    ("andromeda-rpc-protocol", "tokio"),
    ("andromeda-wal", "andromeda-storage"),
    ("andromeda-wal", "andromeda-exec"),
    ("andromeda-wal", "andromeda-execution"),
    ("andromeda-wal", "andromeda-srpl"),
    ("andromeda-wal", "andromeda-quic"),
    ("andromeda-wal", "andromeda-rpc-runtime"),
    ("andromeda-wal", "andromeda-runtime-quinn"),
    ("andromeda-wal", "quinn"),
    ("andromeda-wal", "rustls"),
    ("andromeda-wal", "tokio-rustls"),
    ("andromeda-wal", "h2"),
    ("andromeda-wal", "hyper"),
    ("andromeda-wal", "tower"),
    ("andromeda-wal", "andromeda-analytics"),
    ("andromeda-wal", "andromeda-bench"),
    ("andromeda-wal", "andromeda-gpu"),
    ("andromeda-wal", "andromeda-gpu-kernels"),
    ("andromeda-wal", "andromeda-catalog-runtime"),
    ("andromeda-storage", "andromeda-exec"),
    ("andromeda-storage", "andromeda-quic"),
    ("andromeda-storage", "andromeda-rpc-runtime"),
    ("andromeda-storage", "andromeda-runtime-quinn"),
    ("andromeda-storage", "quinn"),
    ("andromeda-storage", "rustls"),
    ("andromeda-storage", "tokio-rustls"),
    ("andromeda-storage", "h2"),
    ("andromeda-storage", "hyper"),
    ("andromeda-storage", "tower"),
    ("andromeda-storage", "andromeda-srpl"),
    ("andromeda-quic", "andromeda-exec"),
    ("andromeda-exec", "andromeda-runtime-quinn"),
    ("andromeda-exec", "quinn"),
    ("andromeda-exec", "rcgen"),
    ("andromeda-exec", "rustls"),
    ("andromeda-exec", "tonic"),
    ("andromeda-exec", "tonic-build"),
    ("andromeda-exec", "tonic-prost"),
    ("andromeda-exec", "tonic-prost-build"),
    ("andromeda-exec", "tonic-transport"),
    ("andromeda-exec", "tonic-web"),
    ("andromeda-exec", "grpc"),
    ("andromeda-exec", "grpcio"),
    ("andromeda-exec", "grpcio-sys"),
    ("andromeda-exec", "serde-json"),
];

const FORBIDDEN_CATALOG_PRODUCTION_DEPS: &[&str] = &[
    "actix",
    "actix-web",
    "async-std",
    "axum",
    "diesel",
    "grpc",
    "grpcio",
    "grpcio-sys",
    "h2",
    "hyper",
    "json",
    "json-rpc",
    "jsonrpc-core",
    "jsonrpsee",
    "mio",
    "mysql",
    "mysql-async",
    "postgres",
    "quinn",
    "reqwest",
    "rusqlite",
    "rustls",
    "sea-orm",
    "sea-query",
    "serde-json",
    "simd-json",
    "smol",
    "sonic-rs",
    "sqlx",
    "tokio",
    "tokio-postgres",
    "tokio-rustls",
    "tonic",
    "tonic-build",
    "tonic-prost",
    "tonic-prost-build",
    "tonic-transport",
    "tonic-web",
    "tower",
    "warp",
];

const FUTURE_APPLICATION_SURFACE_CRATES: &[&str] = &[
    "andromeda-application",
    "andromeda-application-rpc",
    "andromeda-application-surface",
    "andromeda-rpc-application",
    "andromeda-rpc-application-surface",
];

const FORBIDDEN_APPLICATION_SURFACE_RUNTIME_DEPS: &[&str] = &[
    "andromeda-admin",
    "andromeda-admin-runtime",
    "andromeda-administration",
    "andromeda-administration-runtime",
    "andromeda-backup",
    "andromeda-backup-runtime",
    "andromeda-cluster",
    "andromeda-cluster-runtime",
    "andromeda-hadr",
    "andromeda-hadr-runtime",
];
const SECURITY_CRITICAL_PATH_CRATES: &[&str] = &[
    "andromeda-core",
    "andromeda-observe",
    "andromeda-principal",
    "andromeda-proto",
    "andromeda-rpc-protocol",
    "andromeda-security-contract",
];

const FORBIDDEN_SECURITY_CRITICAL_GPU_RUNTIME_DEPS: &[&str] = &[
    "andromeda-analytics",
    "andromeda-bench",
    "andromeda-bench-harness",
    "andromeda-bench-workload",
    "andromeda-columnar",
    "andromeda-gpu",
    "andromeda-gpu-kernels",
    "andromeda-maps",
    "andromeda-simd",
    "andromeda-vector",
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
    "crates/andromeda-principal/src",
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
const FUTURE_SECURITY_CONTRACT_CRATES: &[&str] = &["andromeda-security-contract"];
const SECURITY_CONTRACT_ALLOWED_RUNTIME_FREE_PRODUCTION_DEPS: &[&str] =
    &["andromeda-digest", "andromeda-error", "andromeda-types"];
const FORBIDDEN_SECURITY_CONTRACT_RUNTIME_DEPS: &[&str] = &[
    "andromeda-core",
    "andromeda-principal",
    "andromeda-contract",
    "andromeda-catalog",
    "andromeda-proto",
    "andromeda-rpc-protocol",
    "andromeda-quic",
    "andromeda-observe",
    "andromeda-analytics",
    "andromeda-bench",
    "andromeda-bench-harness",
    "andromeda-bench-workload",
    "andromeda-columnar",
    "andromeda-gpu",
    "andromeda-gpu-kernels",
    "andromeda-maps",
    "andromeda-simd",
    "andromeda-vector",
    "andromeda-rpc-runtime",
    "andromeda-runtime-quinn",
    "andromeda-exec",
    "andromeda-storage",
    "andromeda-wal",
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

const STRICT_PRODUCTION_DEPENDENCY_ALLOWLISTS: &[(&str, &[&str])] = &[
    ("andromeda-error", &[]),
    ("andromeda-digest", &[]),
    ("andromeda-types", &["andromeda-error"]),
    ("andromeda-time", &["andromeda-error"]),
    ("andromeda-hardware", &["andromeda-error"]),
    (
        "andromeda-core",
        &[
            "andromeda-digest",
            "andromeda-error",
            "andromeda-hardware",
            "andromeda-principal",
            "andromeda-security-contract",
            "andromeda-time",
            "andromeda-types",
        ],
    ),
    (
        "andromeda-principal",
        &[
            "andromeda-digest",
            "andromeda-error",
            "andromeda-security-contract",
            "andromeda-types",
        ],
    ),
    ("andromeda-security-contract", &["andromeda-types"]),
    ("andromeda-test-support", &[]),
    (
        "andromeda-contract",
        &[
            "andromeda-digest",
            "andromeda-error",
            "andromeda-procedure-contract",
            "andromeda-structured-object",
            "andromeda-types",
        ],
    ),
    (
        "andromeda-proto",
        &[
            "andromeda-digest",
            "andromeda-error",
            "andromeda-procedure-contract",
            "andromeda-proto-wire",
            "andromeda-structured-object",
            "andromeda-types",
            "prost",
            "prost-build",
            "protoc-bin-vendored",
        ],
    ),
    (
        "andromeda-catalog",
        &[
            "andromeda-catalog-recovery",
            "andromeda-catalog-store",
            "andromeda-decision-trace",
            "andromeda-definition-batch",
            "andromeda-digest",
            "andromeda-error",
            "andromeda-procedure-contract",
            "andromeda-time",
            "andromeda-types",
        ],
    ),
    (
        "andromeda-observe",
        &[
            "andromeda-audit",
            "andromeda-digest",
            "andromeda-error",
            "andromeda-hardware",
            "andromeda-observability",
            "andromeda-types",
        ],
    ),
    (
        "andromeda-quic",
        &[
            "andromeda-digest",
            "andromeda-error",
            "andromeda-principal",
            "andromeda-procedure-contract",
            "andromeda-rpc",
            "andromeda-rpc-codec",
            "andromeda-rpc-protocol",
            "andromeda-types",
        ],
    ),
    (
        "andromeda-exec",
        &[
            "andromeda-admission",
            "andromeda-audit",
            "andromeda-catalog",
            "andromeda-catalog-store",
            "andromeda-definition-batch",
            "andromeda-error",
            "andromeda-execution",
            "andromeda-execution-trace",
            "andromeda-hardware",
            "andromeda-iam",
            "andromeda-observe",
            "andromeda-observability",
            "andromeda-plan-cache",
            "andromeda-principal",
            "andromeda-procedure-contract",
            "andromeda-procedure-runtime",
            "andromeda-procedure-store",
            "andromeda-quic",
            "andromeda-rpc-protocol",
            "andromeda-result-stream",
            "andromeda-retry",
            "andromeda-security",
            "andromeda-srpl-execution-adapter",
            "andromeda-srpl-interpreter",
            "andromeda-srpl-ir",
            "andromeda-storage",
            "andromeda-storage-heap",
            "andromeda-storage-page",
            "andromeda-storage-placement",
            "andromeda-time",
            "andromeda-mvcc",
            "andromeda-transaction",
            "andromeda-transaction-log",
            "andromeda-types",
            "andromeda-wal",
            "tokio",
        ],
    ),
];

const STRICT_DEV_DEPENDENCY_ALLOWLISTS: &[(&str, &[&str])] = &[
    ("andromeda-error", &[]),
    ("andromeda-digest", &[]),
    ("andromeda-types", &[]),
    ("andromeda-time", &[]),
    ("andromeda-hardware", &[]),
    ("andromeda-principal", &[]),
    ("andromeda-core", &[]),
    ("andromeda-test-support", &[]),
    ("andromeda-contract", &[]),
    ("andromeda-structured-object", &[]),
    ("andromeda-security-contract", &[]),
    (
        "andromeda-proto",
        &["andromeda-rpc-protocol", "prost-types", "proptest"],
    ),
    (
        "andromeda-catalog",
        &[
            "andromeda-business-fixtures",
            "andromeda-catalog-diff",
            "andromeda-contract",
            "andromeda-observe",
            "andromeda-plan-cache",
            "andromeda-scenario-evidence",
            "andromeda-statistics",
            "andromeda-wal",
        ],
    ),
    (
        "andromeda-observe",
        &["andromeda-storage-page", "andromeda-storage-placement"],
    ),
    (
        "andromeda-quic",
        &["andromeda-proto", "andromeda-security-contract", "proptest"],
    ),
    (
        "andromeda-exec",
        &[
            "andromeda-business-fixtures",
            "andromeda-inventory-demo",
            "andromeda-manifest",
            "andromeda-proto",
            "andromeda-recovery",
            "andromeda-srpl",
            "andromeda-srpl-binder",
        ],
    ),
    ("andromeda-srpl-diagnostics", &[]),
    ("andromeda-srpl-cardinality", &[]),
    ("andromeda-srpl-ast", &[]),
    ("andromeda-srpl-parser", &[]),
    ("andromeda-srpl-ir", &[]),
    (
        "andromeda-srpl",
        &[
            "andromeda-contract",
            "andromeda-srpl-catalog-binding",
            "andromeda-srpl-definition-batch",
            "andromeda-srpl-ast",
            "andromeda-srpl-cardinality",
            "andromeda-srpl-execution-adapter",
            "andromeda-srpl-lexer",
            "proptest",
        ],
    ),
];

#[test]
fn dependency_guard_enforces_workspace_doctrine() {
    let workspace = workspace_root();
    let manifests = collect_dependency_manifests(&workspace);
    let mut violations = Vec::new();

    for crate_name in manifests.keys() {
        if let Some(bucket) = crate_name_uses_generic_topology_bucket(crate_name) {
            violations.push(format!(
                "crate name uses generic topology bucket `{bucket}` instead of an ownership boundary: {crate_name}"
            ));
        }
    }

    for (source, allowed_deps) in STRICT_PRODUCTION_DEPENDENCY_ALLOWLISTS {
        let Some(manifest) = manifests.get(*source) else {
            violations.push(format!(
                "missing manifest for strictly allowlisted crate: {source}"
            ));
            continue;
        };

        for dep in &manifest.production_deps {
            if !allowed_deps.contains(&dep.as_str()) {
                violations.push(format!(
                    "strict production dependency allowlist violation: {source} -> {dep}"
                ));
            }
        }
    }

    for (source, allowed_deps) in STRICT_DEV_DEPENDENCY_ALLOWLISTS {
        let Some(manifest) = manifests.get(*source) else {
            violations.push(format!(
                "missing manifest for dev-dependency allowlisted crate: {source}"
            ));
            continue;
        };

        for dep in &manifest.dev_deps {
            if !allowed_deps.contains(&dep.as_str()) {
                violations.push(format!(
                    "strict dev-dependency allowlist violation: {source} -> {dep}"
                ));
            }
        }
    }

    for (source, target) in FORBIDDEN_PRODUCTION_EDGES {
        if let Some(manifest) = manifests.get(*source) {
            if manifest.production_deps.contains(*target) {
                violations.push(format!(
                    "forbidden production dependency edge: {source} -> {target}"
                ));
            }
        } else {
            violations.push(format!("missing manifest for guarded crate: {source}"));
        }
    }

    for source in FUTURE_APPLICATION_SURFACE_CRATES {
        let Some(manifest) = manifests.get(*source) else {
            continue;
        };

        for target in FORBIDDEN_APPLICATION_SURFACE_RUNTIME_DEPS {
            if manifest.production_deps.contains(*target) {
                violations.push(format!(
                    "application RPC surface must not gain administration, cluster, HA/DR, or backup runtime edge: {source} -> {target}"
                ));
            }
        }
    }

    for source in FUTURE_SECURITY_CONTRACT_CRATES {
        let Some(manifest) = manifests.get(*source) else {
            continue;
        };

        for dep in &manifest.production_deps {
            if !SECURITY_CONTRACT_ALLOWED_RUNTIME_FREE_PRODUCTION_DEPS.contains(&dep.as_str()) {
                violations.push(format!(
                    "security contract crate must only use runtime-free foundation production dependencies: {source} -> {dep}"
                ));
            }
        }

        for target in FORBIDDEN_SECURITY_CONTRACT_RUNTIME_DEPS {
            if manifest.production_deps.contains(*target) {
                violations.push(format!(
                    "security contract crate must not gain RPC, execution, durable storage, WAL, transaction, TLS/QUIC, or async runtime edge: {source} -> {target}"
                ));
            }
        }
    }

    for source in SECURITY_CRITICAL_PATH_CRATES {
        let Some(manifest) = manifests.get(*source) else {
            continue;
        };

        for target in FORBIDDEN_SECURITY_CRITICAL_GPU_RUNTIME_DEPS {
            if manifest.production_deps.contains(*target) {
                violations.push(format!(
                    "security-critical path must not gain GPU runtime dependency: {source} -> {target}"
                ));
            }
        }
    }

    violations.extend(security_critical_source_cast_violations(&workspace));

    if let Some(catalog) = manifests.get("andromeda-catalog") {
        for dep in &catalog.production_deps {
            if FORBIDDEN_CATALOG_PRODUCTION_DEPS.contains(&dep.as_str()) {
                violations.push(format!(
                    "catalog must not gain production network/runtime dependency: andromeda-catalog -> {dep}"
                ));
            }
        }
    } else {
        violations.push("missing manifest for guarded crate: andromeda-catalog".to_string());
    }

    for manifest in manifests.values() {
        violations.extend(manifest.forbidden_wire_deps.iter().cloned());
    }
    let workspace_manifest = std::fs::read_to_string(workspace.join("Cargo.toml"))
        .expect("read workspace Cargo.toml for dependency guard");
    violations
        .extend(parse_dependency_manifest("workspace", &workspace_manifest).forbidden_wire_deps);

    assert!(
        violations.is_empty(),
        "dependency doctrine violations detected:\n  - {}",
        violations.join("\n  - ")
    );
}

fn security_critical_source_cast_violations(workspace: &Path) -> Vec<String> {
    let mut violations = Vec::new();

    for root in SECURITY_CRITICAL_SOURCE_ROOTS {
        let root = workspace.join(root);
        if !root.is_dir() {
            continue;
        }

        for file in rust_source_files(&root) {
            let source = fs::read_to_string(&file)
                .unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
            let source = strip_rust_comments(&source);
            let relative = workspace_relative_path(workspace, &file);

            for (line_index, line) in source.lines().enumerate() {
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

    violations
}

fn security_critical_cast_uses_implicit_ordinal(compact_line: &str) -> bool {
    compact_line.contains("asu8")
        && SECURITY_CRITICAL_CAST_CONTEXT_TOKENS
            .iter()
            .any(|token| compact_line.contains(token))
}

fn rust_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_source_files(root, &mut files);
    files.sort();
    files
}

fn collect_rust_source_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|err| panic!("read {}: {err}", dir.display())) {
        let entry = entry.unwrap_or_else(|err| panic!("read entry in {}: {err}", dir.display()));
        let path = entry.path();
        if path.is_dir() {
            collect_rust_source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn workspace_relative_path(workspace: &Path, path: &Path) -> String {
    path.strip_prefix(workspace)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn strip_rust_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut block_depth = 0_usize;
    let mut in_line_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                output.push('\n');
            } else {
                output.push(' ');
            }
            continue;
        }

        if block_depth > 0 {
            match (ch, chars.peek().copied()) {
                ('/', Some('*')) => {
                    chars.next();
                    block_depth += 1;
                    output.push_str("  ");
                },
                ('*', Some('/')) => {
                    chars.next();
                    block_depth -= 1;
                    output.push_str("  ");
                },
                ('\n', _) => output.push('\n'),
                _ => output.push(' '),
            }
            continue;
        }

        match (ch, chars.peek().copied()) {
            ('/', Some('/')) => {
                chars.next();
                in_line_comment = true;
                output.push_str("  ");
            },
            ('/', Some('*')) => {
                chars.next();
                block_depth = 1;
                output.push_str("  ");
            },
            _ => output.push(ch),
        }
    }

    output
}

#[test]
fn dependency_guard_detects_synthetic_forbidden_edges_and_wire_deps() {
    let manifest = parse_dependency_manifest(
        "andromeda-transaction",
        r#"
[dependencies]
andromeda-core.workspace = true
store = { package = "andromeda-storage", workspace = true }
serde_json = "1"

[dev-dependencies]
andromeda-storage.workspace = true

[build-dependencies]
tonic-build = "0.12"
"#,
    );

    assert!(manifest.production_deps.contains("andromeda-storage"));
    assert!(manifest.dev_deps.contains("andromeda-storage"));
    assert!(
        manifest
            .forbidden_wire_deps
            .iter()
            .any(|violation| violation.contains("dependency names: tonic-build"))
    );
    assert!(
        manifest
            .forbidden_wire_deps
            .iter()
            .any(|violation| violation.contains("serde-json"))
    );
}
#[test]
fn dependency_guard_resolves_workspace_aliases_before_forbidden_rules() {
    let workspace_aliases = WorkspaceDependencyAliases::from_manifest(
        r#"
[workspace.dependencies]
transport = { package = "quinn", version = "0.11" }
grpc_wire = { package = "tonic", version = "0.12" }
json_wire = { package = "serde_json", version = "1" }
sql_backend = { package = "sqlx", version = "0.8" }
native_layout = { package = "bytemuck", version = "1" }
store_alias = { package = "andromeda-storage", path = "crates/andromeda-storage" }
"#,
    );
    let manifest = parse_dependency_manifest_with_aliases(
        "andromeda-transaction",
        r#"
[dependencies]
transport.workspace = true
grpc_wire.workspace = true
json_wire.workspace = true
sql_backend.workspace = true
native_layout.workspace = true

[dependencies.store_alias]
workspace = true
"#,
        &workspace_aliases,
    );

    assert!(manifest.production_deps.contains("quinn"));
    assert!(manifest.production_deps.contains("tonic"));
    assert!(manifest.production_deps.contains("serde-json"));
    assert!(manifest.production_deps.contains("sqlx"));
    assert!(manifest.production_deps.contains("bytemuck"));
    assert!(manifest.production_deps.contains("andromeda-storage"));
    assert!(
        manifest
            .forbidden_wire_deps
            .iter()
            .any(|violation| violation.contains("dependency names: grpc-wire, tonic"))
    );
    assert!(
        manifest
            .forbidden_wire_deps
            .iter()
            .any(|violation| violation.contains("serde-json"))
    );
    assert!(
        manifest
            .forbidden_wire_deps
            .iter()
            .any(|violation| violation.contains("sqlx"))
    );
    assert!(
        manifest
            .forbidden_wire_deps
            .iter()
            .any(|violation| violation.contains("bytemuck"))
    );
}
#[test]
fn dependency_guard_detects_synthetic_table_style_forbidden_dependencies() {
    let manifest = parse_dependency_manifest(
        "andromeda-transaction",
        r#"
[dependencies.store]
package = "andromeda-storage"
workspace = true

[dependencies.tonic]
version = "0.12"

[dependencies.json_wire]
package = "serde_json"
version = "1"

[dev-dependencies.andromeda-storage]
workspace = true
"#,
    );

    assert!(manifest.production_deps.contains("andromeda-storage"));
    assert!(manifest.production_deps.contains("tonic"));
    assert!(manifest.dev_deps.contains("andromeda-storage"));
    assert!(
        manifest
            .forbidden_wire_deps
            .iter()
            .any(|violation| violation.contains("dependency names: tonic"))
    );
    assert!(
        manifest
            .forbidden_wire_deps
            .iter()
            .any(|violation| violation.contains("serde-json"))
    );
}
