#![forbid(unsafe_code)]

use andromeda_catalog_diff::{
    CatalogObjectDiff, CatalogObjectDiffImpact, CatalogObjectDiffKind, CatalogObjectDiffSeverity,
    diff_catalog_object_definitions,
};
use andromeda_catalog_store::{
    CatalogDefinition, CatalogObjectRef, ObjectKind, QualifiedName, TableDefinition,
};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ScalarType, TypeDescriptor,
};

fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal,
    }
}

fn table(
    object_id: u64,
    name: &str,
    kind: ObjectKind,
    catalog_version: u64,
    columns: Vec<ColumnDescriptor>,
) -> CatalogDefinition {
    CatalogDefinition::Table(TableDefinition {
        object: CatalogObjectRef {
            object_id: CatalogObjectId::new(object_id),
            name: QualifiedName::parse(name).expect("valid qualified name"),
            kind,
            catalog_version: CatalogVersion::new(catalog_version),
        },
        columns,
    })
}

fn product_table(version: u64, columns: Vec<ColumnDescriptor>) -> CatalogDefinition {
    table(1, "Inventory.Product", ObjectKind::Table, version, columns)
}

#[test]
fn additive_table_shape_change_is_current_catalog_diff_evidence() {
    let previous = product_table(1, vec![column("ProductId", 0)]);
    let next = product_table(
        2,
        vec![column("ProductId", 0), column("QuantityAvailable", 1)],
    );

    let diff = diff_catalog_object_definitions(Some(&previous), Some(&next))
        .expect("shape change must produce catalog diff evidence");

    assert_eq!(diff.kind, CatalogObjectDiffKind::Replaced);
    assert_eq!(diff.severity, CatalogObjectDiffSeverity::WalRequired);
    assert!(diff.requires_durable_wal());
    assert!(
        diff.impacts
            .contains(&CatalogObjectDiffImpact::CatalogVersionChanged)
    );
    assert!(
        diff.impacts
            .contains(&CatalogObjectDiffImpact::ContractHashChanged)
    );
    assert_ne!(diff.previous_shape_hash, diff.next_shape_hash);
}

#[test]
fn object_identity_or_name_change_requires_breaking_review() {
    let previous = product_table(1, vec![column("ProductId", 0)]);
    let next = table(
        2,
        "Inventory.ProductArchive",
        ObjectKind::Table,
        2,
        vec![column("ProductId", 0)],
    );

    let diff = CatalogObjectDiff::between(Some(&previous), Some(&next))
        .expect("identity and name change must produce diff evidence");

    assert_eq!(diff.kind, CatalogObjectDiffKind::Replaced);
    assert_eq!(diff.severity, CatalogObjectDiffSeverity::BreakingReview);
    assert!(diff.requires_durable_wal());
    assert!(
        diff.impacts
            .contains(&CatalogObjectDiffImpact::ObjectIdentityChanged)
    );
    assert!(
        diff.impacts
            .contains(&CatalogObjectDiffImpact::QualifiedNameChanged)
    );
}

#[test]
fn lifecycle_changes_are_wal_required() {
    let definition = product_table(1, vec![column("ProductId", 0)]);

    let added = diff_catalog_object_definitions(None, Some(&definition))
        .expect("object addition must produce diff evidence");
    assert_eq!(added.kind, CatalogObjectDiffKind::Added);
    assert_eq!(added.severity, CatalogObjectDiffSeverity::WalRequired);
    assert!(
        added
            .impacts
            .contains(&CatalogObjectDiffImpact::LifecycleChanged)
    );

    let removed = diff_catalog_object_definitions(Some(&definition), None)
        .expect("object removal must produce diff evidence");
    assert_eq!(removed.kind, CatalogObjectDiffKind::Removed);
    assert_eq!(removed.severity, CatalogObjectDiffSeverity::WalRequired);
    assert!(
        removed
            .impacts
            .contains(&CatalogObjectDiffImpact::LifecycleChanged)
    );
}
