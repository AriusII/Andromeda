use super::common::*;

#[test]
fn catalog_api_has_no_storage_or_exec_coupling() {
    let manifest = include_str!("../../Cargo.toml");
    let runtime_dependencies = manifest
        .split_once("[dev-dependencies]")
        .map_or(manifest, |(runtime_dependencies, _)| runtime_dependencies);

    assert!(!runtime_dependencies.contains("andromeda-storage"));
    assert!(!runtime_dependencies.contains("andromeda-exec"));

    let store = store_at(1);
    assert_eq!(
        store.snapshot().publication,
        CatalogSnapshotPublication::InMemoryOnly
    );
}
