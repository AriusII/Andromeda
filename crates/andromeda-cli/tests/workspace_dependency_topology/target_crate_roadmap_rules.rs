use crate::{
    diagnostics::{relative_slash_path, rust_source_files, strip_rust_comments},
    graph_rules::{
        TemporaryDependencyException, assert_temporary_exceptions_have_exit_criteria,
        dev_dependency_back_edges, temporary_exception_edges,
    },
    manifest_loading::{load_crate_manifests, normalize_dependency_name},
    workspace_root,
};
use std::{collections::BTreeSet, fs};
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
    "andromeda-transaction",
    "andromeda-transaction-log",
    "andromeda-tx",
    "andromeda-wal",
    "andromeda-wal-codec",
];
const FORBIDDEN_GENERIC_CRATE_NAME_PARTS: &[&str] = &["common", "utils", "misc", "helpers"];
const FORBIDDEN_C5_ANALYTICS_GPU_BENCH_DEPS: &[&str] = &[
    "andromeda-bench",
    "andromeda-bench-harness",
    "andromeda-bench-workload",
    "andromeda-analytics",
    "andromeda-columnar",
    "andromeda-gpu",
    "andromeda-gpu-kernels",
    "andromeda-maps",
    "andromeda-simd",
    "andromeda-vector",
];
const FORBIDDEN_C5_ANALYTICS_GPU_BENCH_SOURCE_TOKENS: &[&str] = &[
    "andromeda_analytics::",
    "andromeda_bench::",
    "andromeda_bench_harness::",
    "andromeda_bench_workload::",
    "andromeda_columnar::",
    "andromeda_gpu::",
    "andromeda_gpu_kernels::",
    "andromeda_maps::",
    "andromeda_simd::",
    "andromeda_vector::",
];
const RUNTIME_CRITICAL_CRATES_TO_AUDIT: &[&str] = &[
    "andromeda-exec",
    "andromeda-tx",
    "andromeda-storage",
    "andromeda-wal",
    "andromeda-recovery",
    "andromeda-admin",
    "andromeda-security",
];
const FORBIDDEN_RUNTIME_CRITICAL_SQL_DEPS: &[&str] = &[
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
    "postgres",
    "postcard",
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
const FORBIDDEN_RUNTIME_CRITICAL_SQL_SOURCE_TOKENS: &[&str] = &[
    "abomonation",
    "bincode",
    "bitcode",
    "borsh",
    "bytemuck",
    "diesel",
    "jsonrpc",
    "jsonrpc_core",
    "jsonrpsee",
    "mysql",
    "postgres",
    "postgresql",
    "rusqlite",
    "sea_orm",
    "sea_query",
    "serde_json",
    "simd_json",
    "sonic_rs",
    "speedy",
    "sqlx",
    "tokio_postgres",
];
const FORBIDDEN_RUNTIME_CRITICAL_GPU_DEPS: &[&str] = &[
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
const FORBIDDEN_RUNTIME_CRITICAL_GPU_SOURCE_TOKENS: &[&str] = &[
    "andromeda_analytics",
    "andromeda_bench",
    "andromeda_bench_harness",
    "andromeda_bench_workload",
    "andromeda_columnar",
    "andromeda_gpu",
    "andromeda_gpu_kernels",
    "andromeda_maps",
    "andromeda_simd",
    "andromeda_vector",
    "ash",
    "cuda",
    "cudarc",
    "cust",
    "metal",
    "naga",
    "nvml_wrapper",
    "opencl3",
    "vulkano",
    "wgpu",
];
const FORBIDDEN_RUNTIME_CRITICAL_PROTO_DEPS: &[&str] = &[
    "andromeda-proto-wire",
    "andromeda-proto",
    "andromeda-rpc-protocol",
    "andromeda-rpc-contract",
    "prost",
    "prost-build",
    "prost-types",
    "protoc-bin-vendored",
    "prost-reflect",
    "prost-derive",
];
const FORBIDDEN_RUNTIME_CRITICAL_PROTO_SOURCE_TOKENS: &[&str] = &[
    "andromeda_proto_wire",
    "andromeda_proto",
    "andromeda_rpc_protocol",
    "prost",
    "prost_types",
    "prost_derive",
];
const ALLOWED_RUNTIME_CRITICAL_PROTO_DEPENDENCIES: &[(&str, &[&str])] =
    &[("andromeda-exec", &["andromeda-proto"])];
const ALLOWED_RUNTIME_CRITICAL_PROTO_SOURCE_IMPORTS: &[(&str, &[&str])] =
    &[("andromeda-exec", &["andromeda_proto::"])];
const TEMPORARY_DEV_DEPENDENCY_BACKEDGE_EXCEPTIONS: &[TemporaryDependencyException] = &[
    TemporaryDependencyException {
        source: "andromeda-observe",
        dependency: "andromeda-storage",
        exit_criteria: "Exit criteria: move durable audit storage fixtures into an acyclic test-support crate or remove observe's dev-dependency on storage.",
    },
];
const TEMPORARY_C5_CORE_FACADE_EXCEPTIONS: &[TemporaryDependencyException] = &[
    TemporaryDependencyException {
        source: "andromeda-storage",
        dependency: "andromeda-core",
        exit_criteria: "Exit criteria: replace the wide andromeda-core facade with extracted durable-storage foundation crates.",
    },
    TemporaryDependencyException {
        source: "andromeda-tx",
        dependency: "andromeda-core",
        exit_criteria: "Exit criteria: replace the wide andromeda-core facade with extracted transaction foundation crates.",
    },
    TemporaryDependencyException {
        source: "andromeda-wal",
        dependency: "andromeda-core",
        exit_criteria: "Exit criteria: replace the wide andromeda-core facade with extracted WAL foundation crates.",
    },
    TemporaryDependencyException {
        source: "andromeda-manifest",
        dependency: "andromeda-core",
        exit_criteria: "Exit criteria: move manifest identity and digest foundations behind dedicated durable manifest contracts.",
    },
    TemporaryDependencyException {
        source: "andromeda-savepoint",
        dependency: "andromeda-core",
        exit_criteria: "Exit criteria: extract savepoint identifiers and validation errors from the temporary core facade.",
    },
    TemporaryDependencyException {
        source: "andromeda-segment",
        dependency: "andromeda-core",
        exit_criteria: "Exit criteria: extract segment identity and WAL-safe primitives from the temporary core facade.",
    },
    TemporaryDependencyException {
        source: "andromeda-transaction-log",
        dependency: "andromeda-core",
        exit_criteria: "Exit criteria: extract transaction log identity and replay errors from the temporary core facade.",
    },
];
#[test]
fn core_facade_does_not_grow_new_local_modules_during_foundation_migration() {
    let core_src = workspace_root().join("crates/andromeda-core/src");
    let actual_entries = fs::read_dir(&core_src)
        .expect("read andromeda-core src directory")
        .map(|entry| {
            entry
                .expect("read andromeda-core src entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<BTreeSet<_>>();
    let expected_entries = ["lib.rs", "principal"]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();

    assert_eq!(
        actual_entries, expected_entries,
        "andromeda-core must stay a temporary facade in batch 1; add new foundation code to the extracted crates or document a later-batch migration exception"
    );
}
#[test]
fn workspace_crate_names_reject_generic_topology_buckets() {
    let manifests = load_crate_manifests(&workspace_root().join("crates"));
    let violations = manifests
        .values()
        .filter_map(|manifest| {
            forbidden_generic_crate_name_bucket(&manifest.package_name).map(|bucket| {
                format!(
                    "{} declares anti-pattern crate bucket `{bucket}` in {}; use a responsibility-owned crate name instead of common/utils/misc/helpers/god_engine",
                    manifest.package_name,
                    manifest.path.display()
                )
            })
        })
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "workspace crate names must encode ownership instead of generic buckets:\n{}",
        violations.join("\n")
    );
}
#[test]
fn temporary_dev_dependency_back_edges_are_named_and_bounded() {
    let manifests = load_crate_manifests(&workspace_root().join("crates"));
    let actual = dev_dependency_back_edges(&manifests);
    let expected = temporary_exception_edges(TEMPORARY_DEV_DEPENDENCY_BACKEDGE_EXCEPTIONS);

    assert_temporary_exceptions_have_exit_criteria(
        "dev dependency back-edge",
        TEMPORARY_DEV_DEPENDENCY_BACKEDGE_EXCEPTIONS,
    );
    assert_eq!(
        actual, expected,
        "workspace dev-dependency back-edges must be explicit temporary read-only findings with exit criteria; update the named exception list only when the topology risk is intentionally accepted or removed"
    );
}
#[test]
fn catalog_proto_watch_edge_is_direct_one_way_and_named() {
    let manifests = load_crate_manifests(&workspace_root().join("crates"));
    let catalog = manifests
        .get("andromeda-catalog")
        .expect("workspace must include andromeda-catalog");
    let proto = manifests
        .get("andromeda-proto")
        .expect("workspace must include andromeda-proto");

    assert!(
        catalog.runtime_dependencies.contains("andromeda-proto"),
        "andromeda-catalog -> andromeda-proto is a named watch edge while catalog descriptors still publish protocol-facing schema manifests; remove this assertion when catalog/proto contracts split"
    );
    assert!(
        !proto.runtime_dependencies.contains("andromeda-catalog")
            && !proto.dev_dependencies.contains("andromeda-catalog"),
        "andromeda-proto must not depend back on andromeda-catalog; the catalog -> proto watch edge must stay one-way"
    );
}
#[test]
fn c5_core_facade_imports_are_temporary_named_exceptions() {
    let manifests = load_crate_manifests(&workspace_root().join("crates"));
    let actual = C5_DURABLE_KERNEL_CRATES
        .iter()
        .filter_map(|crate_name| manifests.get(*crate_name))
        .filter(|manifest| manifest.runtime_dependencies.contains("andromeda-core"))
        .map(|manifest| (manifest.package_name.clone(), "andromeda-core".to_owned()))
        .collect::<BTreeSet<_>>();
    let expected = temporary_exception_edges(TEMPORARY_C5_CORE_FACADE_EXCEPTIONS);

    assert_temporary_exceptions_have_exit_criteria(
        "C5 core facade import",
        TEMPORARY_C5_CORE_FACADE_EXCEPTIONS,
    );
    assert_eq!(
        actual, expected,
        "C5 crates importing the wide andromeda-core facade must be explicit temporary exceptions with exit criteria; add durable-kernel foundation crates before widening this list"
    );
}
#[test]
fn c5_crates_do_not_import_gpu_analytics_or_bench_even_for_tests() {
    let workspace = workspace_root();
    let manifests = load_crate_manifests(&workspace.join("crates"));
    let mut violations = Vec::new();

    for crate_name in C5_DURABLE_KERNEL_CRATES {
        let Some(manifest) = manifests.get(*crate_name) else {
            continue;
        };

        for dependency in manifest
            .runtime_dependencies
            .iter()
            .chain(manifest.dev_dependencies.iter())
        {
            if FORBIDDEN_C5_ANALYTICS_GPU_BENCH_DEPS.contains(&dependency.as_str()) {
                violations.push(format!(
                    "{} must not depend on `{dependency}` in any runtime or dev scope in {}",
                    manifest.package_name,
                    manifest.path.display()
                ));
            }
        }

        let source_root = workspace.join("crates").join(*crate_name).join("src");
        if !source_root.is_dir() {
            continue;
        }

        for file in rust_source_files(&source_root) {
            let source = fs::read_to_string(&file)
                .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
            let code_without_comments = strip_rust_comments(&source);
            let relative = relative_slash_path(&workspace, &file);

            for (line_index, line) in code_without_comments.lines().enumerate() {
                let compact_line = line
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect::<String>();

                for token in FORBIDDEN_C5_ANALYTICS_GPU_BENCH_SOURCE_TOKENS {
                    if compact_line.contains(token) {
                        violations.push(format!(
                            "{relative}:{} imports `{token}` from a C5 crate; GPU, analytics, and bench code must stay outside durable kernel paths",
                            line_index + 1
                        ));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "C5 crates must not import GPU, analytics, or benchmark crates:\n{}",
        violations.join("\n")
    );
}
#[test]
fn roadmap_runtime_critical_crates_do_not_import_sql_proto_gpu_domains() {
    let workspace = workspace_root();
    let manifests = load_crate_manifests(&workspace.join("crates"));
    let mut violations = Vec::new();

    for crate_name in RUNTIME_CRITICAL_CRATES_TO_AUDIT {
        let Some(manifest) = manifests.get(*crate_name) else {
            violations.push(format!(
                "roadmap runtime-critical crate `{crate_name}` is missing from workspace manifests"
            ));
            continue;
        };

        for dependency in manifest
            .runtime_dependencies
            .iter()
            .chain(manifest.dev_dependencies.iter())
        {
            if FORBIDDEN_RUNTIME_CRITICAL_SQL_DEPS.contains(&dependency.as_str()) {
                violations.push(format!(
                    "{crate_name} must not depend on SQL/native-layout crate `{dependency}` in {}",
                    manifest.path.display()
                ));
            }

            if FORBIDDEN_RUNTIME_CRITICAL_GPU_DEPS.contains(&dependency.as_str()) {
                violations.push(format!(
                    "{crate_name} must not depend on GPU analytics/bench crate `{dependency}` in {}",
                    manifest.path.display()
                ));
            }

            if FORBIDDEN_RUNTIME_CRITICAL_PROTO_DEPS.contains(&dependency.as_str())
                && !is_allowed_runtime_critical_proto_dependency(crate_name, dependency)
            {
                violations.push(format!(
                    "{crate_name} must not depend on proto domain crate `{dependency}` in {}",
                    manifest.path.display()
                ));
            }
        }

        let source_root = workspace.join("crates").join(*crate_name).join("src");
        if !source_root.is_dir() {
            continue;
        }

        for file in rust_source_files(&source_root) {
            let source = fs::read_to_string(&file)
                .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
            let code_without_comments = strip_rust_comments(&source);
            let relative = relative_slash_path(&workspace, &file);

            for (line_index, line) in code_without_comments.lines().enumerate() {
                let compact_line = line
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect::<String>();

                for token in FORBIDDEN_RUNTIME_CRITICAL_SQL_SOURCE_TOKENS {
                    if source_line_contains_token(&compact_line, token) {
                        violations.push(format!(
                            "{relative}:{} imports SQL/native-layout token `{token}`",
                            line_index + 1
                        ));
                    }
                }

                for token in FORBIDDEN_RUNTIME_CRITICAL_GPU_SOURCE_TOKENS {
                    if source_line_contains_token(&compact_line, token) {
                        violations.push(format!(
                            "{relative}:{} imports GPU/analytics/bench token `{token}`",
                            line_index + 1
                        ));
                    }
                }

                for token in FORBIDDEN_RUNTIME_CRITICAL_PROTO_SOURCE_TOKENS {
                    if source_line_contains_token(&compact_line, token)
                        && !is_allowed_runtime_critical_proto_source_token(crate_name, token)
                    {
                        violations.push(format!(
                            "{relative}:{} imports proto token `{token}`",
                            line_index + 1
                        ));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "roadmap runtime-critical crates must not import forbidden SQL/proto/GPU domain tokens:\n{}",
        violations.join("\n")
    );
}

fn is_allowed_runtime_critical_proto_dependency(crate_name: &str, dependency: &str) -> bool {
    ALLOWED_RUNTIME_CRITICAL_PROTO_DEPENDENCIES
        .iter()
        .any(|(allowed_crate, allowed_tokens)| {
            *allowed_crate == crate_name && allowed_tokens.contains(&dependency)
        })
}

fn is_allowed_runtime_critical_proto_source_token(crate_name: &str, token: &str) -> bool {
    ALLOWED_RUNTIME_CRITICAL_PROTO_SOURCE_IMPORTS
        .iter()
        .any(|(allowed_crate, allowed_tokens)| {
            *allowed_crate == crate_name && allowed_tokens.contains(&token)
        })
}

fn source_line_contains_token(line: &str, token: &str) -> bool {
    line.split(|character: char| {
        !character.is_ascii_alphanumeric() && character != '_' && character != '-'
    })
    .any(|segment| segment == token)
}

fn forbidden_generic_crate_name_bucket(package_name: &str) -> Option<&'static str> {
    let normalized = normalize_dependency_name(package_name);
    let ownership_name = normalized
        .strip_prefix("andromeda-")
        .unwrap_or(normalized.as_str());

    for &bucket in FORBIDDEN_GENERIC_CRATE_NAME_PARTS {
        if ownership_name.split('-').any(|part| part == bucket) {
            return Some(bucket);
        }
    }

    if ownership_name.contains("god-engine") || ownership_name.contains("godengine") {
        return Some("god_engine");
    }

    None
}
#[test]
fn crate_name_guard_rejects_generic_topology_buckets() {
    assert_eq!(
        forbidden_generic_crate_name_bucket("andromeda-common"),
        Some("common")
    );
    assert_eq!(
        forbidden_generic_crate_name_bucket("andromeda-runtime-utils"),
        Some("utils")
    );
    assert_eq!(
        forbidden_generic_crate_name_bucket("andromeda-god_engine"),
        Some("god_engine")
    );
    assert_eq!(forbidden_generic_crate_name_bucket("andromeda-wal"), None);
}
