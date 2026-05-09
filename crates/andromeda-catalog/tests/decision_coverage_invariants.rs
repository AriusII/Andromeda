use andromeda_catalog_store::{
    CatalogDefinition, CatalogObjectRef, ObjectKind, QualifiedName, TableDefinition,
};
use andromeda_definition_batch::{CatalogLifecycleTarget, DefinitionOperation};
use andromeda_types::{CatalogObjectId, CatalogVersion};

fn catalog_spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("specs")
        .join("catalog-srpl.md")
}

#[test]
fn catalog_spec_covers_alter_procedure_lifecycle_before_operation_surface_expands() {
    let spec =
        std::fs::read_to_string(catalog_spec_path()).expect("catalog spec must be readable");
    let normalized = spec.split_whitespace().collect::<Vec<_>>().join(" ");

    for required in [
        "Alter | Existing target, explicit identity preservation, compatibility acceptance.",
        "Input changes, required permission changes, result stream removal",
        "contract, catalog, stats, and policy evidence allow reuse",
        "Compatibility tests must classify alters, drops, renames, moves",
    ] {
        assert!(
            normalized.contains(required),
            "catalog-srpl.md must cover required Alter Procedure topic: {required}"
        );
    }
}

#[test]
fn catalog_spec_covers_drop_procedure_lifecycle_before_operation_surface_expands() {
    let spec =
        std::fs::read_to_string(catalog_spec_path()).expect("catalog spec must be readable");
    let normalized = spec
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    for required in [
        "Drop or deprecate",
        "dependency closure",
        "active invocation policy",
        "historical evidence retention",
        "Deprecated | Fence keys for new invocations of the deprecated version.",
        "Reject new invocation binding to the deprecated active name or version.",
    ] {
        assert!(
            normalized.contains(&required.to_lowercase()),
            "catalog-srpl.md must cover required Drop Procedure topic: {required}"
        );
    }
}

#[test]
fn drop_procedure_operation_surface_is_guarded_not_yet_implemented() {
    let create_example = DefinitionOperation::Create(CatalogDefinition::Table(TableDefinition {
        object: CatalogObjectRef {
            object_id: CatalogObjectId::new(1),
            name: QualifiedName::parse("test.Table").unwrap(),
            kind: ObjectKind::Table,
            catalog_version: CatalogVersion::new(1),
        },
        columns: vec![],
    }));

    let deprecate_example = DefinitionOperation::Deprecate(CatalogLifecycleTarget {
        object: CatalogObjectRef {
            object_id: CatalogObjectId::new(1),
            name: QualifiedName::parse("test.Table").unwrap(),
            kind: ObjectKind::Table,
            catalog_version: CatalogVersion::new(1),
        },
    });

    match (&create_example, &deprecate_example) {
        (DefinitionOperation::Create(_), DefinitionOperation::Deprecate(_)) => {},
        _ => {
            panic!(
                "DefinitionOperation surface must remain limited to Create/Deprecate until Drop is explicitly designed and decided"
            );
        },
    }
}

#[test]
fn drop_procedure_publication_policy_is_constrained_by_catalog_spec() {
    let spec =
        std::fs::read_to_string(catalog_spec_path()).expect("catalog spec must be readable");
    let normalized = spec
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    assert!(
        normalized.contains("no operation from a partially failed batch may become visible"),
        "catalog spec must keep DefinitionBatch apply all-or-nothing"
    );

    assert!(
        normalized.contains("visible publication requires durable wal coverage"),
        "catalog spec must keep drop/deprecate publication behind durable WAL"
    );

    assert!(
        normalized.contains("dependency closure"),
        "catalog spec must keep drop/deprecate dependency closure explicit"
    );
}
