use super::common::*;

#[test]
fn snapshot_state_rejects_catalog_identity_mismatch() {
    let store = store_at(10);
    let operation = create_inventory_product(11);

    let mut wrong_database = batch(version(10), vec![operation.clone()]);
    wrong_database.database_id = DatabaseId::new(DATABASE_ID.get() + 100);
    let error = store.plan_definition_batch(&wrong_database).unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("database id"));

    let mut wrong_namespace = batch(version(10), vec![operation]);
    wrong_namespace.namespace_id = NamespaceId::new(NAMESPACE_ID.get() + 100);
    let error = store.plan_definition_batch(&wrong_namespace).unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("namespace id"));
}

#[test]
fn snapshot_state_rejects_create_collisions_with_existing_objects() {
    let mut store = store_at(10);
    store
        .apply_definition_batch(&product_batch(10, 11))
        .unwrap();

    let duplicate_id_error = store
        .plan_definition_batch(&batch(
            version(11),
            vec![create_table(1, "Inventory.Stock", 12)],
        ))
        .unwrap_err();
    assert_eq!(duplicate_id_error.kind(), AndromedaErrorKind::Catalog);
    assert!(duplicate_id_error.message().contains("already present"));
    assert!(duplicate_id_error.message().contains("object id"));

    let duplicate_name_error = store
        .plan_definition_batch(&batch(
            version(11),
            vec![create_table(2, "Inventory.Product", 12)],
        ))
        .unwrap_err();
    assert_eq!(duplicate_name_error.kind(), AndromedaErrorKind::Catalog);
    assert!(duplicate_name_error.message().contains("already present"));
    assert!(duplicate_name_error.message().contains("object name"));
}

#[test]
fn snapshot_state_rejects_deprecation_of_missing_target() {
    let store = store_at(10);

    let error = store
        .plan_definition_batch(&batch(
            version(10),
            vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                object: object(99, "Inventory.Missing", ObjectKind::Table, version(10)),
            })],
        ))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("unknown object id"));
}

#[test]
fn dependency_ordering_is_rejected_for_late_structured_inputs() {
    let store = store_at(10);

    let error = store
        .plan_definition_batch(&batch(
            version(10),
            vec![
                DefinitionOperation::Create(CatalogDefinition::Procedure(procedure(
                    2,
                    "Inventory.ReserveStock",
                    version(11),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ))),
                DefinitionOperation::Create(CatalogDefinition::StructuredObject(
                    structured_object(1, "Inventory.StockRequest", version(11)),
                )),
            ],
        ))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("dependencies before dependent"));
}

#[test]
fn snapshot_state_validates_external_structured_input_dependencies() {
    let missing_store = store_at(10);
    let error = missing_store
        .plan_definition_batch(&batch(
            version(10),
            vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
                procedure(
                    2,
                    "Inventory.ReserveStock",
                    version(11),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ),
            ))],
        ))
        .unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("dependency"));
    assert!(error.message().contains("missing"));

    let mut store = store_at(10);
    store
        .apply_definition_batch(&batch(
            version(10),
            vec![DefinitionOperation::Create(
                CatalogDefinition::StructuredObject(structured_object(
                    1,
                    "Inventory.StockRequest",
                    version(11),
                )),
            )],
        ))
        .unwrap();

    let plan = store
        .plan_definition_batch(&batch(
            version(11),
            vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
                procedure(
                    2,
                    "Inventory.ReserveStock",
                    version(12),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ),
            ))],
        ))
        .unwrap();

    assert_eq!(plan.previous_version, version(11));
    assert_eq!(plan.next_version, version(12));
    assert_eq!(plan.created_objects.len(), 1);
}

#[test]
fn procedure_structured_inputs_are_catalog_dependencies() {
    let contract = procedure(
        2,
        "Inventory.ReserveStock",
        version(11),
        vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
    );
    let definition = CatalogDefinition::Procedure(contract);

    let dependencies = definition.dependencies();

    assert_eq!(dependencies.len(), 1);
    assert_eq!(
        dependencies[0].kind,
        CatalogDependencyKind::ProcedureStructuredInput
    );
    assert_eq!(dependencies[0].dependent_kind, ObjectKind::Procedure);
    assert_eq!(
        dependencies[0].dependency_kind,
        ObjectKind::StructuredObject
    );
    assert_eq!(
        dependencies[0].dependency_name,
        QualifiedName::parse("Inventory.StockRequest").unwrap()
    );
}

#[test]
fn snapshot_state_rejects_deprecation_with_active_dependents() {
    let mut store = store_at(10);
    store
        .apply_definition_batch(&batch(
            version(10),
            vec![DefinitionOperation::Create(
                CatalogDefinition::StructuredObject(structured_object(
                    1,
                    "Inventory.StockRequest",
                    version(11),
                )),
            )],
        ))
        .unwrap();
    store
        .apply_definition_batch(&batch(
            version(11),
            vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
                procedure(
                    2,
                    "Inventory.ReserveStock",
                    version(12),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ),
            ))],
        ))
        .unwrap();

    let error = store
        .plan_definition_batch(&batch(
            version(12),
            vec![DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                object: object(
                    1,
                    "Inventory.StockRequest",
                    ObjectKind::StructuredObject,
                    version(11),
                ),
            })],
        ))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("active dependents"));
}

#[test]
fn snapshot_state_allows_joint_deprecation_of_dependency_and_dependent() {
    let mut store = store_at(10);
    store
        .apply_definition_batch(&batch(
            version(10),
            vec![DefinitionOperation::Create(
                CatalogDefinition::StructuredObject(structured_object(
                    1,
                    "Inventory.StockRequest",
                    version(11),
                )),
            )],
        ))
        .unwrap();
    store
        .apply_definition_batch(&batch(
            version(11),
            vec![DefinitionOperation::Create(CatalogDefinition::Procedure(
                procedure(
                    2,
                    "Inventory.ReserveStock",
                    version(12),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ),
            ))],
        ))
        .unwrap();

    let plan = store
        .plan_definition_batch(&batch(
            version(12),
            vec![
                DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                    object: object(
                        2,
                        "Inventory.ReserveStock",
                        ObjectKind::Procedure,
                        version(12),
                    ),
                }),
                DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                    object: object(
                        1,
                        "Inventory.StockRequest",
                        ObjectKind::StructuredObject,
                        version(11),
                    ),
                }),
            ],
        ))
        .unwrap();

    assert_eq!(plan.deprecated_objects.len(), 2);
    assert_eq!(plan.next_version, version(13));
}

#[test]
fn snapshot_state_rejects_new_dependent_when_dependency_is_deprecated() {
    let mut store = store_at(10);
    store
        .apply_definition_batch(&batch(
            version(10),
            vec![DefinitionOperation::Create(
                CatalogDefinition::StructuredObject(structured_object(
                    1,
                    "Inventory.StockRequest",
                    version(11),
                )),
            )],
        ))
        .unwrap();

    let error = store
        .plan_definition_batch(&batch(
            version(11),
            vec![
                DefinitionOperation::Create(CatalogDefinition::Procedure(procedure(
                    2,
                    "Inventory.ReserveStock",
                    version(12),
                    vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
                ))),
                DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                    object: object(
                        1,
                        "Inventory.StockRequest",
                        ObjectKind::StructuredObject,
                        version(11),
                    ),
                }),
            ],
        ))
        .unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("active dependents"));
}
