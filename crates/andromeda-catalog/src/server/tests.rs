use super::*;
use andromeda_core::{CatalogVersion, ProcedureId};

#[test]
fn test_procedure_manifest_validation() {
    // Valid manifest
    let valid = ProcedureManifest {
        procedure_id: ProcedureId::new(1),
        qualified_name: "public.my_proc".to_string(),
        catalog_version: CatalogVersion::new(1),
        contract_hash: vec![1, 2, 3],
        input_schema: vec![],
        output_schema: vec![],
        is_mutable: false,
        min_compatible_version: CatalogVersion::new(1),
    };
    assert!(valid.validate().is_ok());

    // Invalid: zero procedure ID
    let invalid_id = ProcedureManifest {
        procedure_id: ProcedureId::new(0),
        ..valid.clone()
    };
    assert!(invalid_id.validate().is_err());

    // Invalid: empty qualified name
    let invalid_name = ProcedureManifest {
        qualified_name: String::new(),
        ..valid.clone()
    };
    assert!(invalid_name.validate().is_err());

    // Invalid: zero catalog version
    let invalid_version = ProcedureManifest {
        catalog_version: CatalogVersion::new(0),
        ..valid.clone()
    };
    assert!(invalid_version.validate().is_err());

    // Invalid: empty contract hash
    let invalid_hash = ProcedureManifest {
        contract_hash: vec![],
        ..valid.clone()
    };
    assert!(invalid_hash.validate().is_err());
}

#[test]
fn test_column_schema_validation() {
    // Valid column
    let valid = ColumnSchema {
        name: "id".to_string(),
        type_descriptor: "int64".to_string(),
        ordinal: 0,
        nullable: false,
    };
    assert!(valid.validate().is_ok());

    // Invalid: empty name
    let invalid_name = ColumnSchema {
        name: String::new(),
        ..valid.clone()
    };
    assert!(invalid_name.validate().is_err());

    // Invalid: empty type descriptor
    let invalid_type = ColumnSchema {
        type_descriptor: String::new(),
        ..valid.clone()
    };
    assert!(invalid_type.validate().is_err());
}

#[test]
fn test_mock_catalog_server_procedure_resolution() {
    let server = MockCatalogServer::new();
    let proc_id = ProcedureId::new(1);

    // Should fail for unknown procedure
    let result = server.resolve_procedure(proc_id);
    assert!(result.is_err());

    // Register a procedure
    let manifest = ProcedureManifest {
        procedure_id: proc_id,
        qualified_name: "public.my_proc".to_string(),
        catalog_version: CatalogVersion::new(1),
        contract_hash: vec![1, 2, 3],
        input_schema: vec![ColumnSchema {
            name: "input_col".to_string(),
            type_descriptor: "int64".to_string(),
            ordinal: 0,
            nullable: false,
        }],
        output_schema: vec![ColumnSchema {
            name: "result".to_string(),
            type_descriptor: "text".to_string(),
            ordinal: 0,
            nullable: true,
        }],
        is_mutable: false,
        min_compatible_version: CatalogVersion::new(1),
    };

    assert!(server.register_procedure(manifest.clone()).is_ok());

    // Now resolution should succeed
    let resolved = server.resolve_procedure(proc_id).unwrap();
    assert_eq!(resolved.qualified_name, "public.my_proc");
    assert_eq!(resolved.procedure_id, proc_id);
}

#[test]
fn test_mock_catalog_server_version_tracking() {
    let server = MockCatalogServer::new();

    let initial_version = server.get_catalog_version();
    assert_eq!(initial_version.get(), 1);

    // Advance version
    assert!(server.advance_catalog_version(100).is_ok());
    let new_version = server.get_catalog_version();
    assert_eq!(new_version.get(), 2);

    // Advance again
    assert!(server.advance_catalog_version(200).is_ok());
    let newer_version = server.get_catalog_version();
    assert_eq!(newer_version.get(), 3);
}

#[test]
fn test_mock_catalog_server_change_subscription() {
    let server = MockCatalogServer::new();

    // Advance version multiple times
    assert!(server.advance_catalog_version(100).is_ok());
    assert!(server.advance_catalog_version(200).is_ok());
    assert!(server.advance_catalog_version(300).is_ok());

    // Subscribe to changes
    let mut subscription = server.subscribe_to_changes().unwrap();
    assert!(subscription.is_active());

    // Poll changes
    let change1 = subscription.next_change();
    assert!(change1.is_some());
    let c1 = change1.unwrap();
    assert_eq!(c1.new_version.get(), 2);
    assert_eq!(c1.previous_version.get(), 1);
    assert_eq!(c1.invalidation_boundary_lsn, 100);

    let change2 = subscription.next_change();
    assert!(change2.is_some());
    let c2 = change2.unwrap();
    assert_eq!(c2.new_version.get(), 3);
    assert_eq!(c2.previous_version.get(), 2);
    assert_eq!(c2.invalidation_boundary_lsn, 200);

    let change3 = subscription.next_change();
    assert!(change3.is_some());
    let c3 = change3.unwrap();
    assert_eq!(c3.new_version.get(), 4);
    assert_eq!(c3.previous_version.get(), 3);
    assert_eq!(c3.invalidation_boundary_lsn, 300);

    // No more changes
    let change4 = subscription.next_change();
    assert!(change4.is_none());

    subscription.close();
    assert!(!subscription.is_active());
}

#[test]
fn test_catalog_change_notification_affects_procedure() {
    let notification = CatalogChangeNotification {
        new_version: CatalogVersion::new(2),
        previous_version: CatalogVersion::new(1),
        invalidation_boundary_lsn: 100,
    };

    // Mock implementation considers any version change as affecting all procedures
    assert!(notification.affects_procedure(ProcedureId::new(1)));
    assert!(notification.affects_procedure(ProcedureId::new(999)));

    let no_change = CatalogChangeNotification {
        new_version: CatalogVersion::new(1),
        previous_version: CatalogVersion::new(1),
        invalidation_boundary_lsn: 100,
    };
    assert!(!no_change.affects_procedure(ProcedureId::new(1)));
}
