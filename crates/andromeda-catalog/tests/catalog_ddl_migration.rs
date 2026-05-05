use andromeda_catalog::{
    CatalogDefinition, CatalogDdlMigrationAction, CatalogDdlMigrationClassification,
    CatalogLifecycleTarget, CatalogObjectRef, CatalogPublicationSemantics, DefinitionBatch,
    DefinitionBatchId, DefinitionOperation, ObjectKind, QualifiedName, TableDefinition,
};
use andromeda_core::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, DatabaseId, NamespaceId, ScalarType,
    TypeDescriptor,
};

const DATABASE_ID: DatabaseId = DatabaseId::new(1);
const NAMESPACE_ID: NamespaceId = NamespaceId::new(2);

fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal,
    }
}

fn object(id: u64, name: &str, kind: ObjectKind, version: CatalogVersion) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(id),
        name: QualifiedName::parse(name).unwrap(),
        kind,
        catalog_version: version,
    }
}

fn table(id: u64, name: &str, version: CatalogVersion) -> TableDefinition {
    TableDefinition {
        object: object(id, name, ObjectKind::Table, version),
        columns: vec![column("ProductId", 0)],
    }
}

fn batch(base_version: CatalogVersion, operations: Vec<DefinitionOperation>) -> DefinitionBatch {
    DefinitionBatch {
        batch_id: DefinitionBatchId::new(base_version.get() + 100),
        database_id: DATABASE_ID,
        namespace_id: NAMESPACE_ID,
        base_version,
        operations,
    }
}

#[test]
fn catalog_ddl_migration_report_tracks_wal_and_publication_doctrine() {
    let plan = batch(
        CatalogVersion::new(10),
        vec![DefinitionOperation::Create(CatalogDefinition::Table(table(
            1,
            "Inventory.Product",
            CatalogVersion::new(11),
        )))],
    )
    .dry_run()
    .unwrap();

    let ddl_plan = plan.catalog_ddl_migration_plan();

    assert!(ddl_plan.is_catalog_only());
    assert_eq!(ddl_plan.previous_version, CatalogVersion::new(10));
    assert_eq!(ddl_plan.next_version, CatalogVersion::new(11));
    assert_eq!(ddl_plan.wal_record_count, plan.mutation_plan.record_count());
    assert_eq!(
        ddl_plan.publication_semantics,
        CatalogPublicationSemantics::DurablePublicationExternal
    );
    assert_eq!(ddl_plan.catalog_operations.len(), 1);
    assert_eq!(
        ddl_plan.catalog_operations[0].classification,
        CatalogDdlMigrationClassification::CatalogDdlMigration
    );
    assert_eq!(
        ddl_plan.catalog_operations[0].action,
        CatalogDdlMigrationAction::CreateObject
    );
}

#[test]
fn catalog_ddl_migration_report_separates_procedure_lifecycle_deprecation() {
    let procedure_target = CatalogLifecycleTarget {
        object: object(
            7,
            "Inventory.ReserveStock",
            ObjectKind::Procedure,
            CatalogVersion::new(10),
        ),
    };
    let plan = batch(
        CatalogVersion::new(10),
        vec![DefinitionOperation::Deprecate(procedure_target)],
    )
    .dry_run()
    .unwrap();

    let ddl_plan = plan.catalog_ddl_migration_plan();

    assert!(!ddl_plan.is_catalog_only());
    assert!(ddl_plan.catalog_operations.is_empty());
    assert_eq!(ddl_plan.procedure_lifecycle_operations.len(), 1);
    assert_eq!(
        ddl_plan.procedure_lifecycle_operations[0].classification,
        CatalogDdlMigrationClassification::SrplProcedureLifecycle
    );
    assert_eq!(
        ddl_plan.procedure_lifecycle_operations[0].action,
        CatalogDdlMigrationAction::DeprecateObject
    );
}
