use std::{collections::BTreeSet, fs, path::Path};

use super::super::support::{
    CRATE_LOCAL_PROTO_SCHEMAS, collect_proto_files_under, collect_proto_relative_paths,
    manifest_dir,
};

#[test]
fn governed_proto_registry_matches_crate_local_tree() {
    let manifest_dir = manifest_dir();
    let proto_root = manifest_dir.join("proto");
    let discovered = collect_proto_relative_paths(&proto_root);
    let governed = CRATE_LOCAL_PROTO_SCHEMAS
        .iter()
        .map(|schema| schema.relative_path)
        .collect::<BTreeSet<_>>();

    assert_eq!(
        discovered, governed,
        "schema governance must include every crate-local .proto file and no external sources"
    );

    for schema in CRATE_LOCAL_PROTO_SCHEMAS {
        let crate_local_source = fs::read_to_string(manifest_dir.join(schema.relative_path))
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", schema.relative_path));
        assert_eq!(
            schema.source, crate_local_source,
            "{} must be included from the crate-local proto tree",
            schema.relative_path
        );
    }
}

#[test]
fn repository_root_schemas_proto_tree_remains_retired() {
    let manifest_dir = manifest_dir();
    let repository_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("andromeda-proto must remain under <repo>/crates/andromeda-proto");
    let legacy_proto_root = repository_root.join("schemas").join("proto");
    let legacy_proto_files = collect_proto_files_under(&legacy_proto_root);

    assert!(
        legacy_proto_files.is_empty(),
        "schemas/proto is retired; crates/andromeda-proto/proto/andromeda/** is the sole \
         protobuf source authority. Remove stale legacy files or add an explicit deterministic \
         mirror drift contract before retaining a mirror. Legacy files discovered: {legacy_proto_files:?}"
    );
}
