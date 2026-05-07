use crate::dependency_manifest::{collect_dependency_manifests, parse_dependency_manifest};
use crate::support::workspace_root;

const FORBIDDEN_PRODUCTION_EDGES: &[(&str, &str)] = &[
    ("andromeda-tx", "andromeda-storage"),
    ("andromeda-wal", "andromeda-storage"),
    ("andromeda-wal", "andromeda-tx"),
    ("andromeda-wal", "andromeda-exec"),
    ("andromeda-wal", "andromeda-execution"),
    ("andromeda-wal", "andromeda-srpl"),
    ("andromeda-wal", "andromeda-quic"),
    ("andromeda-wal", "andromeda-rpc-runtime"),
    ("andromeda-wal", "andromeda-runtime-quinn"),
    ("andromeda-wal", "andromeda-analytics"),
    ("andromeda-wal", "andromeda-bench"),
    ("andromeda-wal", "andromeda-gpu"),
    ("andromeda-wal", "andromeda-gpu-kernels"),
    ("andromeda-wal", "andromeda-catalog-runtime"),
    ("andromeda-storage", "andromeda-exec"),
    ("andromeda-storage", "andromeda-quic"),
    ("andromeda-storage", "andromeda-srpl"),
    ("andromeda-quic", "andromeda-exec"),
];

const FORBIDDEN_CATALOG_PRODUCTION_DEPS: &[&str] = &[
    "actix",
    "actix-web",
    "async-std",
    "axum",
    "h2",
    "hyper",
    "mio",
    "quinn",
    "reqwest",
    "rustls",
    "smol",
    "tokio",
    "tokio-rustls",
    "tonic",
    "tower",
    "warp",
];

#[test]
fn dependency_guard_enforces_workspace_doctrine() {
    let workspace = workspace_root();
    let manifests = collect_dependency_manifests(&workspace);
    let mut violations = Vec::new();

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

#[test]
fn dependency_guard_detects_synthetic_forbidden_edges_and_wire_deps() {
    let manifest = parse_dependency_manifest(
        "andromeda-tx",
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
            .any(|violation| violation.contains("serde_json"))
    );
}

#[test]
fn dependency_guard_detects_synthetic_table_style_forbidden_dependencies() {
    let manifest = parse_dependency_manifest(
        "andromeda-tx",
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
            .any(|violation| violation.contains("serde_json"))
    );
}
