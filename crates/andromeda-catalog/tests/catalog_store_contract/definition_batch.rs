use super::common::*;

#[test]
fn duplicate_object_ids_and_names_are_rejected() {
    let store = store_at(10);

    let duplicate_id_error = store
        .plan_definition_batch(&batch(
            version(10),
            vec![
                create_table(1, "Inventory.Product", 11),
                create_table(1, "Inventory.Stock", 11),
            ],
        ))
        .unwrap_err();
    assert_eq!(duplicate_id_error.kind(), AndromedaErrorKind::Catalog);
    assert!(duplicate_id_error.message().contains("object id"));

    let duplicate_name_error = store
        .plan_definition_batch(&batch(
            version(10),
            vec![
                create_table(1, "Inventory.Product", 11),
                create_table(2, "Inventory.Product", 11),
            ],
        ))
        .unwrap_err();
    assert_eq!(duplicate_name_error.kind(), AndromedaErrorKind::Catalog);
    assert!(duplicate_name_error.message().contains("object name"));
}

#[test]
fn definition_batch_source_hash_preserves_ordered_source_identity() {
    let base_version = version(10);
    let first = batch(
        base_version,
        vec![
            create_table(1, "Inventory.Product", 11),
            create_table(2, "Inventory.Stock", 11),
        ],
    );
    let reordered = batch(
        base_version,
        vec![
            create_table(2, "Inventory.Stock", 11),
            create_table(1, "Inventory.Product", 11),
        ],
    );

    assert_eq!(first.source_hash(), first.clone().source_hash());
    assert!(!first.source_hash().is_zero());
    assert_ne!(
        first.source_hash(),
        reordered.source_hash(),
        "source hash must bind the exact ordered DefinitionBatch source"
    );

    assert_eq!(
        first.dependency_graph_hash().unwrap(),
        reordered.dependency_graph_hash().unwrap(),
        "independent operations produce the same canonical dependency graph"
    );
}

#[test]
fn dependency_graph_hash_changes_with_catalog_dependency_edge() {
    let base_version = version(20);
    let catalog_version = version(21);
    let stock_request = CatalogDefinition::StructuredObject(structured_object(
        1,
        "Inventory.StockRequest",
        catalog_version,
    ));
    let audit_request = CatalogDefinition::StructuredObject(structured_object(
        2,
        "Inventory.AuditRequest",
        catalog_version,
    ));

    let stock_dependency = batch(
        base_version,
        vec![
            DefinitionOperation::Create(stock_request.clone()),
            DefinitionOperation::Create(audit_request.clone()),
            DefinitionOperation::Create(CatalogDefinition::Procedure(procedure(
                3,
                "Inventory.ReserveStock",
                catalog_version,
                vec![QualifiedName::parse("Inventory.StockRequest").unwrap()],
            ))),
        ],
    );
    let audit_dependency = batch(
        base_version,
        vec![
            DefinitionOperation::Create(stock_request),
            DefinitionOperation::Create(audit_request),
            DefinitionOperation::Create(CatalogDefinition::Procedure(procedure(
                3,
                "Inventory.ReserveStock",
                catalog_version,
                vec![QualifiedName::parse("Inventory.AuditRequest").unwrap()],
            ))),
        ],
    );

    let stock_hash = stock_dependency.dependency_graph_hash().unwrap();
    let audit_hash = audit_dependency.dependency_graph_hash().unwrap();

    assert!(!stock_hash.is_zero());
    assert!(!audit_hash.is_zero());
    assert_ne!(
        stock_hash, audit_hash,
        "dependency graph hash must bind the canonical dependency edge set"
    );
}
