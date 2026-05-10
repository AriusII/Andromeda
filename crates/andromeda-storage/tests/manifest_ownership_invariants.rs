#![forbid(unsafe_code)]

use std::path::PathBuf;

#[test]
fn pure_manifest_facade_file_stays_demolished() {
    let workspace = workspace_root();
    let demolished_facades = [
        "crates/andromeda-storage/src/manifest.rs",
        "crates/andromeda-storage/src/manifest/tests.rs",
    ];

    let mut failures = Vec::new();
    for relative in demolished_facades {
        if workspace.join(relative).exists() {
            failures.push(format!(
                "{relative} reintroduced a storage manifest facade file; import manifest contracts from andromeda-manifest"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "manifest facade demolition regressions:\n  - {}",
        failures.join("\n  - ")
    );
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root resolves")
}
