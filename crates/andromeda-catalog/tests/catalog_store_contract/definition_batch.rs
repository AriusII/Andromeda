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
