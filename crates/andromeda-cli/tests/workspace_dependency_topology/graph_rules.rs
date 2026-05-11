use crate::{
    forbidden_dependencies::{
        FORBIDDEN_SECURITY_CRITICAL_GPU_RUNTIME_DEPS, SECURITY_CONTRACT_FORBIDDEN_RUNTIME_DEPS,
        SECURITY_CRITICAL_PATH_CRATES,
    },
    manifest_loading::{
        CrateManifest, collect_crate_directory_manifests, collect_workspace_member_manifests,
        load_crate_manifests,
    },
    workspace_root,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};
const WORKSPACE_CRATE_COUNT: usize = 90;
const C5_DURABLE_KERNEL_CRATES: &[&str] = &[
    "andromeda-backup",
    "andromeda-buffer-pool",
    "andromeda-disk-page-store",
    "andromeda-hadr",
    "andromeda-locking",
    "andromeda-manifest",
    "andromeda-mvcc",
    "andromeda-recovery",
    "andromeda-restore",
    "andromeda-savepoint",
    "andromeda-segment",
    "andromeda-storage",
    "andromeda-storage-heap",
    "andromeda-storage-index",
    "andromeda-storage-page",
    "andromeda-storage-placement",
    "andromeda-transaction",
    "andromeda-transaction-log",
    "andromeda-wal",
    "andromeda-wal-codec",
];
const FORBIDDEN_CRITICAL_PATH_ACCELERATION_DEPS: &[&str] = &[
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
];
const FORBIDDEN_C5_PROTOCOL_RUNTIME_DEPS: &[&str] = &[
    "andromeda-exec",
    "andromeda-execution",
    "andromeda-procedure-runtime",
    "andromeda-quic",
    "andromeda-quic-runtime-quinn",
    "andromeda-rpc-runtime",
    "andromeda-runtime-quinn",
    "h2",
    "hyper",
    "quinn",
    "rcgen",
    "rustls",
    "tokio-rustls",
    "tonic",
    "tonic-build",
    "tonic-health",
    "tonic-prost",
    "tonic-prost-build",
    "tonic-reflection",
    "tonic-transport",
    "tonic-web",
    "tower",
];
const FORBIDDEN_C5_MODEL_AND_STORE_DEPS: &[&str] = &[
    "andromeda-catalog",
    "andromeda-catalog-runtime",
    "andromeda-catalog-store",
    "andromeda-srpl",
    "andromeda-srpl-ast",
    "andromeda-srpl-binder",
    "andromeda-srpl-catalog-binding",
    "andromeda-srpl-cardinality",
    "andromeda-srpl-definition-batch",
    "andromeda-srpl-diagnostics",
    "andromeda-srpl-execution-adapter",
    "andromeda-srpl-interpreter",
    "andromeda-srpl-ir",
    "andromeda-srpl-lexer",
    "andromeda-srpl-lowering",
    "andromeda-srpl-parser",
];
const FORBIDDEN_C5_WIRE_AND_NATIVE_LAYOUT_DEPS: &[&str] = &[
    "abomonation",
    "bincode",
    "bitcode",
    "borsh",
    "bytemuck",
    "diesel",
    "json",
    "json-rpc",
    "jsonrpc",
    "jsonrpc-core",
    "jsonrpsee",
    "mysql",
    "mysql-async",
    "postcard",
    "postgres",
    "rkyv",
    "rusqlite",
    "sea-orm",
    "sea-query",
    "serde",
    "serde-json",
    "serde-json-core",
    "simd-json",
    "sonic-rs",
    "speedy",
    "sqlx",
    "tokio-postgres",
    "zerocopy",
];
#[test]
fn workspace_crate_dependency_topology_blocks_forbidden_runtime_edges() {
    let manifests = load_crate_manifests(&workspace_root().join("crates"));
    let violations = forbidden_rules()
        .iter()
        .flat_map(|rule| rule.violations(&manifests))
        .chain(
            forbidden_rules()
                .iter()
                .flat_map(|rule| rule.transitive_violations(&manifests)),
        )
        .chain(
            allowed_dependency_rules()
                .iter()
                .flat_map(|rule| rule.violations(&manifests)),
        )
        .chain(
            allowed_dev_dependency_rules()
                .iter()
                .flat_map(|rule| rule.violations(&manifests)),
        )
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "workspace dependency topology violations:\n{}",
        violations.join("\n")
    );
}
#[test]
fn workspace_dependency_topology_tracks_current_crate_surface() {
    let workspace = workspace_root();
    let manifests = load_crate_manifests(&workspace.join("crates"));
    let workspace_member_manifests = collect_workspace_member_manifests(&workspace.join("crates"));
    let crate_directory_manifests = collect_crate_directory_manifests(&workspace.join("crates"));

    assert_eq!(
        manifests.len(),
        WORKSPACE_CRATE_COUNT,
        "workspace topology tests must cover the current crate members declared in root Cargo.toml"
    );
    assert_eq!(
        workspace_member_manifests.len(),
        WORKSPACE_CRATE_COUNT,
        "root Cargo.toml workspace.members must stay aligned with the P00 crate-count baseline"
    );
    assert_eq!(
        relative_manifest_paths(&workspace, &workspace_member_manifests),
        relative_manifest_paths(&workspace, &crate_directory_manifests),
        "root Cargo.toml workspace.members must match the physical crate Cargo.toml files under crates/"
    );
}
fn relative_manifest_paths(workspace: &std::path::Path, paths: &[PathBuf]) -> BTreeSet<String> {
    paths
        .iter()
        .map(|path| relative_slash_path_for_manifest(workspace, path))
        .collect()
}
fn relative_slash_path_for_manifest(workspace: &std::path::Path, path: &std::path::Path) -> String {
    path.strip_prefix(workspace)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
fn forbidden_rules() -> Vec<ForbiddenRule> {
    vec![
        ForbiddenRule::new(
            "Foundation crates and the temporary core surface must not depend on higher-level engine crates",
            &[
                "andromeda-error",
                "andromeda-digest",
                "andromeda-types",
                "andromeda-time",
                "andromeda-hardware",
                "andromeda-principal",
                "andromeda-core",
            ],
            &[
                "andromeda-catalog",
                "andromeda-srpl",
                "andromeda-proto",
                "andromeda-storage",
                "andromeda-exec",
                "andromeda-observe",
                "andromeda-quic",
                "andromeda-bench",
                "andromeda-analytics",
                "andromeda-gpu",
                "andromeda-gpu-kernels",
                "quinn",
            ],
        ),
        ForbiddenRule::new(
            "C5 and durable kernel crates must not depend on GPU, analytics, or benchmark crates",
            C5_DURABLE_KERNEL_CRATES,
            FORBIDDEN_CRITICAL_PATH_ACCELERATION_DEPS,
        ),
        ForbiddenRule::new(
            "Security-critical path crates must not depend on GPU runtime crates",
            SECURITY_CRITICAL_PATH_CRATES,
            FORBIDDEN_SECURITY_CRITICAL_GPU_RUNTIME_DEPS,
        ),
        ForbiddenRule::new(
            "WAL, storage, and recovery crates must not depend on concrete network runtime or RPC runtime crates",
            C5_DURABLE_KERNEL_CRATES,
            FORBIDDEN_C5_PROTOCOL_RUNTIME_DEPS,
        ),
        ForbiddenRule::new(
            "Durable kernel crates must not depend on SRPL, catalog store, execution, or protocol runtime crates",
            C5_DURABLE_KERNEL_CRATES,
            &[
                FORBIDDEN_C5_MODEL_AND_STORE_DEPS,
                FORBIDDEN_C5_PROTOCOL_RUNTIME_DEPS,
                &[
                    "andromeda-proto",
                    "andromeda-proto-wire",
                    "andromeda-rpc",
                    "andromeda-rpc-codec",
                    "andromeda-rpc-contract",
                    "andromeda-rpc-protocol",
                    "andromeda-rpc-surface",
                    "prost",
                    "prost-build",
                    "prost-types",
                    "protoc-bin-vendored",
                ],
            ]
            .concat(),
        ),
        ForbiddenRule::new(
            "Durable kernel crates must not use implicit native layout, SQL, or runtime JSON serialization dependencies",
            C5_DURABLE_KERNEL_CRATES,
            FORBIDDEN_C5_WIRE_AND_NATIVE_LAYOUT_DEPS,
        ),
        ForbiddenRule::new(
            "Catalog and Procedure contract crates must not depend on execution crates",
            &[
                "andromeda-catalog",
                "andromeda-contract",
                "andromeda-contracts",
                "andromeda-procedure-contract",
            ],
            &["andromeda-exec", "andromeda-execution"],
        ),
        ForbiddenRule::new(
            "Procedure contract crates must not depend on catalog, protocol, runtime, storage, analytics, or wire-generation crates",
            &["andromeda-contract"],
            &[
                "andromeda-core",
                "andromeda-catalog",
                "andromeda-contract",
                "andromeda-proto",
                "andromeda-srpl",
                "andromeda-observe",
                "andromeda-storage",
                "andromeda-exec",
                "andromeda-quic",
                "andromeda-bench",
                "andromeda-analytics",
                "andromeda-gpu",
                "andromeda-gpu-kernels",
                "prost",
                "prost-build",
                "protoc-bin-vendored",
                "quinn",
                "tokio",
            ],
        ),
        ForbiddenRule::new(
            "StructuredObject contract model must not depend on protocol, runtime, storage, analytics, or wire-generation crates",
            &["andromeda-structured-object"],
            &[
                "andromeda-core",
                "andromeda-contract",
                "andromeda-catalog",
                "andromeda-proto",
                "andromeda-srpl",
                "andromeda-observe",
                "andromeda-storage",
                "andromeda-exec",
                "andromeda-quic",
                "andromeda-bench",
                "andromeda-analytics",
                "andromeda-gpu",
                "andromeda-gpu-kernels",
                "prost",
                "prost-types",
                "prost-build",
                "protoc-bin-vendored",
                "quinn",
                "tokio",
                "serde",
                "serde-json",
                "sqlx",
                "rusqlite",
                "diesel",
            ],
        ),
        ForbiddenRule::new(
            "SRPL parser and language-model crates must not depend on catalog store implementation crates",
            &[
                "andromeda-srpl-parser",
                "andromeda-srpl-ast",
                "andromeda-srpl-lexer",
                "andromeda-srpl-diagnostics",
                "andromeda-srpl-cardinality",
                "andromeda-srpl-ir",
            ],
            &[
                "andromeda-catalog",
                "andromeda-catalog-store",
                "andromeda-catalog-runtime",
            ],
        ),
        ForbiddenRule::new(
            "Abstract RPC and protocol contract crates must not depend on concrete network runtime crates",
            &[
                "andromeda-proto",
                "andromeda-protocol-contract",
                "andromeda-rpc",
                "andromeda-rpc-abstract",
                "andromeda-rpc-protocol",
                "andromeda-rpc-contract",
                "andromeda-rpc-surface",
            ],
            &[
                "andromeda-quic",
                "andromeda-runtime-quinn",
                "quinn",
                "rcgen",
                "rustls",
                "tokio",
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
            ],
        ),
        ForbiddenRule::new(
            "Security contract crate must remain runtime-free and below RPC, execution, durable storage, WAL, transaction, TLS/QUIC, and async runtime crates",
            &["andromeda-security-contract"],
            SECURITY_CONTRACT_FORBIDDEN_RUNTIME_DEPS,
        ),
        ForbiddenRule::new(
            "Security runtime must not depend on observe; observe may only keep compatibility reexports above security",
            &["andromeda-security"],
            &["andromeda-observe"],
        ),
        ForbiddenRule::new(
            "Application RPC surface crates must not depend on administration, cluster, HA/DR, or backup runtime crates",
            &[
                "andromeda-application",
                "andromeda-application-rpc",
                "andromeda-application-surface",
                "andromeda-rpc-application",
                "andromeda-rpc-application-surface",
            ],
            &[
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
            ],
        ),
    ]
}
fn allowed_dependency_rules() -> Vec<AllowedDependencyRule> {
    vec![
        AllowedDependencyRule::new(
            "andromeda-error must stay an R0 foundation leaf crate",
            "andromeda-error",
            &[],
        ),
        AllowedDependencyRule::new(
            "andromeda-digest must stay an R0 foundation leaf crate",
            "andromeda-digest",
            &[],
        ),
        AllowedDependencyRule::new(
            "andromeda-types may only depend on R0 foundation error handling",
            "andromeda-types",
            &["andromeda-error"],
        ),
        AllowedDependencyRule::new(
            "andromeda-time may only depend on R0 foundation error handling",
            "andromeda-time",
            &["andromeda-error"],
        ),
        AllowedDependencyRule::new(
            "andromeda-hardware may only depend on R0 foundation error handling",
            "andromeda-hardware",
            &["andromeda-error"],
        ),
        AllowedDependencyRule::new(
            "andromeda-core may only remain a temporary R0 surface over foundation crates and the documented security-contract exception",
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
        AllowedDependencyRule::new(
            "andromeda-principal may only depend on runtime-free principal foundation crates",
            "andromeda-principal",
            &[
                "andromeda-digest",
                "andromeda-error",
                "andromeda-security-contract",
                "andromeda-types",
            ],
        ),
        AllowedDependencyRule::new(
            "andromeda-security may only depend on security contracts, audit payloads, observability IDs, and foundation errors",
            "andromeda-security",
            &[
                "andromeda-audit",
                "andromeda-error",
                "andromeda-observability",
                "andromeda-security-contract",
            ],
        ),
        AllowedDependencyRule::new(
            "andromeda-test-support must remain runtime-free mechanical test support",
            "andromeda-test-support",
            &[],
        ),
        AllowedDependencyRule::new(
            "andromeda-contract may only depend on contract-safe R0 crates",
            "andromeda-contract",
            &[
                "andromeda-digest",
                "andromeda-error",
                "andromeda-procedure-contract",
                "andromeda-structured-object",
                "andromeda-types",
            ],
        ),
        AllowedDependencyRule::new(
            "andromeda-structured-object may only depend on contract-safe R0 crates",
            "andromeda-structured-object",
            &["andromeda-digest", "andromeda-error", "andromeda-types"],
        ),
        AllowedDependencyRule::new(
            "andromeda-srpl-diagnostics may only depend on foundation error handling",
            "andromeda-srpl-diagnostics",
            &["andromeda-error"],
        ),
        AllowedDependencyRule::new(
            "andromeda-srpl-cardinality may only depend on contract-safe cardinality types",
            "andromeda-srpl-cardinality",
            &["andromeda-procedure-contract"],
        ),
        AllowedDependencyRule::new(
            "andromeda-srpl-ast may only depend on parser-safe language model crates",
            "andromeda-srpl-ast",
            &[
                "andromeda-procedure-contract",
                "andromeda-srpl-cardinality",
                "andromeda-srpl-diagnostics",
                "andromeda-types",
            ],
        ),
        AllowedDependencyRule::new(
            "andromeda-srpl-parser may only depend on parser-safe language model crates",
            "andromeda-srpl-parser",
            &[
                "andromeda-procedure-contract",
                "andromeda-srpl-ast",
                "andromeda-srpl-cardinality",
                "andromeda-srpl-diagnostics",
                "andromeda-srpl-lexer",
                "andromeda-types",
            ],
        ),
        AllowedDependencyRule::new(
            "andromeda-srpl-ir may only depend on contract-safe semantic model crates",
            "andromeda-srpl-ir",
            &[
                "andromeda-error",
                "andromeda-procedure-contract",
                "andromeda-srpl-cardinality",
                "andromeda-types",
            ],
        ),
        AllowedDependencyRule::new(
            "andromeda-rpc-protocol may only depend on runtime-free protocol foundation crates",
            "andromeda-rpc-protocol",
            &[
                "andromeda-error",
                "andromeda-procedure-contract",
                "andromeda-types",
            ],
        ),
        AllowedDependencyRule::new(
            "andromeda-security-contract may only depend on runtime-free security contract foundation crates",
            "andromeda-security-contract",
            &["andromeda-types"],
        ),
        AllowedDependencyRule::new(
            "andromeda-proto may only depend on Lot 2 protocol schema and contract-safe foundation crates",
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
        AllowedDependencyRule::new(
            "andromeda-catalog may only depend on Lot 2 catalog contract, typed evidence, and protocol contract crates",
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
        AllowedDependencyRule::new(
            "andromeda-observe may only depend on diagnostic, hardware, and foundation crates",
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
        AllowedDependencyRule::new(
            "andromeda-quic may only depend on abstract protocol/security foundations and optional runtime-quinn crates",
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
        AllowedDependencyRule::new(
            "andromeda-exec may only depend on execution, audit, IAM, security, and direct transaction owner crates",
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
        AllowedDependencyRule::new(
            "andromeda-storage may only depend on current Lot 4.3 durable-kernel support crates",
            "andromeda-storage",
            &[
                "andromeda-buffer-pool",
                "andromeda-catalog-recovery",
                "andromeda-disk-page-store",
                "andromeda-error",
                "andromeda-hadr",
                "andromeda-hardware",
                "andromeda-manifest",
                "andromeda-observe",
                "andromeda-recovery",
                "andromeda-segment",
                "andromeda-storage-heap",
                "andromeda-storage-index",
                "andromeda-storage-page",
                "andromeda-types",
                "andromeda-wal",
                "dashmap",
                "sha2",
            ],
        ),
        AllowedDependencyRule::new(
            "andromeda-wal may only depend on pure WAL foundation crates",
            "andromeda-wal",
            &[
                "andromeda-error",
                "andromeda-types",
                "andromeda-wal-codec",
                "dashmap",
            ],
        ),
    ]
}
fn allowed_dev_dependency_rules() -> Vec<AllowedDependencyRule> {
    vec![
        AllowedDependencyRule::new_for_scope(
            "andromeda-error must not gain dev-dependencies without an R0 topology update",
            "andromeda-error",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-digest must not gain dev-dependencies without an R0 topology update",
            "andromeda-digest",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-types must not gain dev-dependencies without an R0 topology update",
            "andromeda-types",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-time must not gain dev-dependencies without an R0 topology update",
            "andromeda-time",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-hardware must not gain dev-dependencies without an R0 topology update",
            "andromeda-hardware",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-principal must not gain dev-dependencies while it remains a low-level principal owner",
            "andromeda-principal",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-core must not gain dev-dependencies while it remains an R0 surface",
            "andromeda-core",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-test-support must not gain dev-dependencies without a fixture-boundary review",
            "andromeda-test-support",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-contract must not gain dev-dependencies without a Lot 2 contract-governance update",
            "andromeda-contract",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-structured-object must not gain dev-dependencies without a Lot 2 contract-governance update",
            "andromeda-structured-object",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-security-contract must not gain dev-dependencies while it remains runtime-free vocabulary",
            "andromeda-security-contract",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-proto may only dev-depend on Lot 2 schema compatibility and property-test crates",
            "andromeda-proto",
            DependencyScope::Dev,
            &["andromeda-rpc-protocol", "prost-types", "proptest"],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-catalog may only dev-depend on the documented Lot 2 catalog integration harness",
            "andromeda-catalog",
            DependencyScope::Dev,
            &[
                "andromeda-business-fixtures",
                "andromeda-catalog-diff",
                "andromeda-contract",
                "andromeda-observe",
                "andromeda-plan-cache",
                "andromeda-scenario-evidence",
                "andromeda-statistics",
                "andromeda-structured-object",
                "andromeda-wal",
            ],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-observe should not dev-depend on storage placement harnesses",
            "andromeda-observe",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-quic may only dev-depend on property-test harness crates",
            "andromeda-quic",
            DependencyScope::Dev,
            &["andromeda-proto", "andromeda-security-contract", "proptest"],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-exec may only dev-depend on the documented business-fixture harness",
            "andromeda-exec",
            DependencyScope::Dev,
            &[
                "andromeda-business-fixtures",
                "andromeda-inventory-demo",
                "andromeda-manifest",
                "andromeda-proto",
                "andromeda-recovery",
                "andromeda-srpl",
                "andromeda-srpl-binder",
                "andromeda-srpl-definition-batch",
            ],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-srpl-diagnostics must not gain dev-dependencies during Lot 3 language-model extraction",
            "andromeda-srpl-diagnostics",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-srpl-cardinality must not gain dev-dependencies during Lot 3 language-model extraction",
            "andromeda-srpl-cardinality",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-srpl-ast must not gain dev-dependencies during Lot 3 language-model extraction",
            "andromeda-srpl-ast",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-srpl-parser must not gain dev-dependencies during Lot 3 language-model extraction",
            "andromeda-srpl-parser",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-srpl-ir must not gain dev-dependencies during Lot 3 language-model extraction",
            "andromeda-srpl-ir",
            DependencyScope::Dev,
            &[],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-srpl may only dev-depend on the documented Lot 3 compatibility test harness crates",
            "andromeda-srpl",
            DependencyScope::Dev,
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
        AllowedDependencyRule::new_for_scope(
            "andromeda-storage may only dev-depend on storage test harness crates",
            "andromeda-storage",
            DependencyScope::Dev,
            &["proptest", "tempfile"],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-wal may only dev-depend on pure WAL test harness crates",
            "andromeda-wal",
            DependencyScope::Dev,
            &["proptest", "tempfile"],
        ),
    ]
}
#[test]
fn tests_fuzzing_stays_registry_and_corpus_only_not_harness_owner() {
    let workspace = workspace_root();
    let tests_fuzzing_root = workspace.join("tests/fuzzing");
    let registry_path = tests_fuzzing_root.join("targets.toml");
    let corpus_manifest_path = tests_fuzzing_root.join("corpus/manifest.toml");
    let registry_text = fs::read_to_string(&registry_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", registry_path.display()));
    let corpus_manifest_text = fs::read_to_string(&corpus_manifest_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", corpus_manifest_path.display()));

    let rust_files_under_tests_fuzzing =
        collect_relative_rs_files(&tests_fuzzing_root, &tests_fuzzing_root);
    assert!(
        rust_files_under_tests_fuzzing.is_empty(),
        "tests/fuzzing must stay registry/corpus-only; executable Rust harnesses belong under fuzz/fuzz_targets, not tests/fuzzing:\n{}",
        rust_files_under_tests_fuzzing.join("\n")
    );

    let registry_harness_paths = quoted_toml_values(&registry_text, "path");
    let invalid_registry_paths = registry_harness_paths
        .iter()
        .filter(|path| {
            !path.starts_with("fuzz_targets/")
                || !path.ends_with(".rs")
                || !workspace.join("fuzz").join(path).is_file()
        })
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        invalid_registry_paths.is_empty(),
        "tests/fuzzing/targets.toml must reference Rust harness files under fuzz/fuzz_targets only:\n{}",
        invalid_registry_paths.join("\n")
    );

    let invalid_corpus_dirs = quoted_toml_values(&corpus_manifest_text, "corpus_dir")
        .into_iter()
        .filter(|path| !path.starts_with("tests/fuzzing/corpus/") || !workspace.join(path).is_dir())
        .collect::<Vec<_>>();
    assert!(
        invalid_corpus_dirs.is_empty(),
        "tests/fuzzing/corpus/manifest.toml must stay a corpus registry rooted under tests/fuzzing/corpus:\n{}",
        invalid_corpus_dirs.join("\n")
    );
}

fn collect_relative_rs_files(root: &std::path::Path, current: &std::path::Path) -> Vec<String> {
    let mut files = Vec::new();

    for entry in
        fs::read_dir(current).unwrap_or_else(|err| panic!("read {}: {err}", current.display()))
    {
        let entry =
            entry.unwrap_or_else(|err| panic!("read entry in {}: {err}", current.display()));
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_relative_rs_files(root, &path));
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(
                path.strip_prefix(root)
                    .expect("tests/fuzzing file stays under root")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }

    files.sort();
    files
}

fn quoted_toml_values(text: &str, key: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let prefix = format!("{key} = ");
            let value = trimmed.strip_prefix(&prefix)?;
            Some(value.trim().trim_matches('"').replace('\\', "/"))
        })
        .collect()
}

pub(crate) struct TemporaryDependencyException {
    pub(crate) source: &'static str,
    pub(crate) dependency: &'static str,
    pub(crate) exit_criteria: &'static str,
}
pub(crate) fn temporary_exception_edges(
    exceptions: &[TemporaryDependencyException],
) -> BTreeSet<(String, String)> {
    exceptions
        .iter()
        .map(|exception| (exception.source.to_owned(), exception.dependency.to_owned()))
        .collect()
}
pub(crate) fn assert_temporary_exceptions_have_exit_criteria(
    context: &str,
    exceptions: &[TemporaryDependencyException],
) {
    let mut seen_edges = BTreeSet::new();

    for exception in exceptions {
        assert!(
            seen_edges.insert((exception.source, exception.dependency)),
            "{context} exception {} -> {} must be listed only once",
            exception.source,
            exception.dependency
        );
        assert!(
            exception.exit_criteria.starts_with("Exit criteria: ")
                && exception.exit_criteria.len() > "Exit criteria: ".len(),
            "{context} exception {} -> {} must name exit criteria",
            exception.source,
            exception.dependency
        );
    }
}
pub(crate) fn assert_temporary_exception_edges_match(
    context: &str,
    actual: BTreeSet<(String, String)>,
    exceptions: &[TemporaryDependencyException],
    message: &str,
) {
    assert_temporary_exceptions_have_exit_criteria(context, exceptions);
    assert_eq!(actual, temporary_exception_edges(exceptions), "{message}");
}
pub(crate) fn dev_dependency_back_edges(
    manifests: &BTreeMap<String, CrateManifest>,
) -> BTreeSet<(String, String)> {
    let mut back_edges = BTreeSet::new();

    for manifest in manifests.values() {
        for dependency in &manifest.dev_dependencies {
            let Some(dependency_manifest) = manifests.get(dependency) else {
                continue;
            };
            if dependency_manifest
                .runtime_dependencies
                .contains(&manifest.package_name)
            {
                back_edges.insert((manifest.package_name.clone(), dependency.clone()));
            }
        }
    }

    back_edges
}
struct ForbiddenRule {
    message: &'static str,
    sources: BTreeSet<&'static str>,
    forbidden_dependencies: BTreeSet<&'static str>,
}
impl ForbiddenRule {
    fn new(
        message: &'static str,
        sources: &[&'static str],
        forbidden_dependencies: &[&'static str],
    ) -> Self {
        Self {
            message,
            sources: sources.iter().copied().collect(),
            forbidden_dependencies: forbidden_dependencies.iter().copied().collect(),
        }
    }

    fn violations(&self, manifests: &BTreeMap<String, CrateManifest>) -> Vec<String> {
        manifests
            .values()
            .filter(|manifest| self.sources.contains(manifest.package_name.as_str()))
            .flat_map(|manifest| {
                manifest
                    .runtime_dependencies
                    .iter()
                    .filter(|dependency| self.forbidden_dependencies.contains(dependency.as_str()))
                    .map(|dependency| {
                        format!(
                            "{}: `{}` must not depend on `{}` in {}",
                            self.message,
                            manifest.package_name,
                            dependency,
                            manifest.path.display()
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn transitive_violations(&self, manifests: &BTreeMap<String, CrateManifest>) -> Vec<String> {
        manifests
            .values()
            .filter(|manifest| self.sources.contains(manifest.package_name.as_str()))
            .flat_map(|manifest| self.transitive_violations_for_manifest(manifest, manifests))
            .collect()
    }

    fn transitive_violations_for_manifest(
        &self,
        manifest: &CrateManifest,
        manifests: &BTreeMap<String, CrateManifest>,
    ) -> Vec<String> {
        let mut violations = Vec::new();
        let mut visited = BTreeSet::new();
        let mut pending = manifest
            .runtime_dependencies
            .iter()
            .map(|dependency| vec![manifest.package_name.clone(), dependency.clone()])
            .collect::<Vec<_>>();

        while let Some(path) = pending.pop() {
            let dependency = path.last().expect("path contains dependency").clone();

            if path.len() > 2 && self.forbidden_dependencies.contains(dependency.as_str()) {
                if allowed_temporary_transitive_forbidden_path(
                    manifest.package_name.as_str(),
                    dependency.as_str(),
                    &path,
                ) {
                    continue;
                }
                violations.push(format!(
                    "{}: `{}` must not transitively depend on `{}` through `{}` in {}",
                    self.message,
                    manifest.package_name,
                    dependency,
                    path.join(" -> "),
                    manifest.path.display()
                ));
                continue;
            }

            if !visited.insert(dependency.clone()) {
                continue;
            }

            let Some(dependency_manifest) = manifests.get(&dependency) else {
                continue;
            };

            for child_dependency in &dependency_manifest.runtime_dependencies {
                let mut child_path = path.clone();
                child_path.push(child_dependency.clone());
                pending.push(child_path);
            }
        }

        violations
    }
}
fn allowed_temporary_transitive_forbidden_path(
    source: &str,
    dependency: &str,
    path: &[String],
) -> bool {
    source == "andromeda-storage"
        && path
            .get(1)
            .is_some_and(|first_hop| first_hop == "andromeda-catalog-recovery")
        && matches!(
            dependency,
            "andromeda-catalog-store" | "andromeda-srpl-diagnostics"
        )
}
struct AllowedDependencyRule {
    message: &'static str,
    source: &'static str,
    scope: DependencyScope,
    allowed_dependencies: BTreeSet<&'static str>,
}
impl AllowedDependencyRule {
    fn new(
        message: &'static str,
        source: &'static str,
        allowed_dependencies: &[&'static str],
    ) -> Self {
        Self::new_for_scope(
            message,
            source,
            DependencyScope::Runtime,
            allowed_dependencies,
        )
    }

    fn new_for_scope(
        message: &'static str,
        source: &'static str,
        scope: DependencyScope,
        allowed_dependencies: &[&'static str],
    ) -> Self {
        Self {
            message,
            source,
            scope,
            allowed_dependencies: allowed_dependencies.iter().copied().collect(),
        }
    }

    fn violations(&self, manifests: &BTreeMap<String, CrateManifest>) -> Vec<String> {
        let Some(manifest) = manifests.get(self.source) else {
            return Vec::new();
        };

        self.dependencies_for(manifest)
            .iter()
            .filter(|dependency| !self.allowed_dependencies.contains(dependency.as_str()))
            .map(|dependency| {
                format!(
                    "{}: `{}` must not depend on `{}` in {}",
                    self.message,
                    manifest.package_name,
                    dependency,
                    manifest.path.display()
                )
            })
            .collect()
    }

    fn dependencies_for<'a>(&self, manifest: &'a CrateManifest) -> &'a BTreeSet<String> {
        match self.scope {
            DependencyScope::Runtime => &manifest.runtime_dependencies,
            DependencyScope::Dev => &manifest.dev_dependencies,
        }
    }
}
#[derive(Clone, Copy)]
enum DependencyScope {
    Runtime,
    Dev,
}
#[test]
fn forbidden_rules_report_transitive_dependency_paths() {
    let manifests = BTreeMap::from([
        (
            "andromeda-storage".to_owned(),
            test_manifest(
                "andromeda-storage",
                &["andromeda-observe"],
                "crates/andromeda-storage/Cargo.toml",
            ),
        ),
        (
            "andromeda-observe".to_owned(),
            test_manifest(
                "andromeda-observe",
                &["andromeda-quic"],
                "crates/andromeda-observe/Cargo.toml",
            ),
        ),
        (
            "andromeda-quic".to_owned(),
            test_manifest(
                "andromeda-quic",
                &["quinn"],
                "crates/andromeda-quic/Cargo.toml",
            ),
        ),
    ]);
    let rule = ForbiddenRule::new(
        "WAL, storage, and recovery crates must not depend on Quinn or RPC runtime crates",
        &["andromeda-storage"],
        &["andromeda-quic", "quinn"],
    );

    let violations = rule.transitive_violations(&manifests);

    assert_eq!(violations.len(), 1);
    assert!(
        violations[0].contains("andromeda-storage -> andromeda-observe -> andromeda-quic"),
        "{violations:?}"
    );
}
fn test_manifest(name: &str, runtime_dependencies: &[&str], path: &str) -> CrateManifest {
    CrateManifest {
        package_name: name.to_owned(),
        path: PathBuf::from(path),
        runtime_dependencies: runtime_dependencies
            .iter()
            .map(|dependency| dependency.to_string())
            .collect(),
        dev_dependencies: BTreeSet::new(),
    }
}
