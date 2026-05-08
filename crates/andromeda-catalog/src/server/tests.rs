use super::*;
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash, ProcedureId,
};

fn contract_hash(byte: u8) -> Vec<u8> {
    vec![byte; ContractHash::LEN]
}

fn valid_manifest() -> ProcedureManifest {
    ProcedureManifest {
        procedure_id: ProcedureId::new(1),
        qualified_name: "public.my_proc".to_string(),
        catalog_version: CatalogVersion::new(2),
        contract_hash: contract_hash(0xA5),
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
    }
}

struct DiagnosticOnlyCatalogServer {
    diagnostic: CatalogServerRuntimeDiagnostic,
}

impl CatalogServerTrait for DiagnosticOnlyCatalogServer {
    fn runtime_diagnostic(&self) -> CatalogServerRuntimeDiagnostic {
        self.diagnostic
    }

    fn resolve_procedure(&self, procedure_id: ProcedureId) -> AndromedaResult<ProcedureManifest> {
        Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            format!(
                "diagnostic-only catalog server cannot resolve procedure {}",
                procedure_id.get()
            ),
        ))
    }

    fn get_catalog_version(&self) -> CatalogVersion {
        self.diagnostic
            .catalog_version
            .unwrap_or_else(|| CatalogVersion::new(1))
    }

    fn subscribe_to_changes(&self) -> AndromedaResult<Box<dyn CatalogChangeSubscription>> {
        Err(AndromedaError::new(
            AndromedaErrorKind::Catalog,
            "diagnostic-only catalog server does not support subscriptions",
        ))
    }
}

#[test]
fn test_procedure_manifest_validation() {
    let valid = valid_manifest();
    assert!(valid.validate().is_ok());

    let invalid_id = ProcedureManifest {
        procedure_id: ProcedureId::new(0),
        ..valid.clone()
    };
    assert!(invalid_id.validate().is_err());

    let invalid_name = ProcedureManifest {
        qualified_name: " \t ".to_string(),
        ..valid.clone()
    };
    assert!(invalid_name.validate().is_err());

    let invalid_version = ProcedureManifest {
        catalog_version: CatalogVersion::new(0),
        ..valid.clone()
    };
    assert!(invalid_version.validate().is_err());

    let invalid_hash_len = ProcedureManifest {
        contract_hash: vec![],
        ..valid.clone()
    };
    assert!(invalid_hash_len.validate().is_err());

    let zero_hash = ProcedureManifest {
        contract_hash: contract_hash(0),
        ..valid.clone()
    };
    assert!(zero_hash.validate().is_err());

    let incompatible_floor = ProcedureManifest {
        min_compatible_version: CatalogVersion::new(3),
        ..valid.clone()
    };
    assert!(incompatible_floor.validate().is_err());

    let non_contiguous_input_schema = ProcedureManifest {
        input_schema: vec![ColumnSchema {
            name: "input_col".to_string(),
            type_descriptor: "int64".to_string(),
            ordinal: 1,
            nullable: false,
        }],
        ..valid
    };
    assert!(non_contiguous_input_schema.validate().is_err());
}

#[test]
fn test_column_schema_validation() {
    let valid = ColumnSchema {
        name: "id".to_string(),
        type_descriptor: "int64".to_string(),
        ordinal: 0,
        nullable: false,
    };
    assert!(valid.validate().is_ok());

    let invalid_name = ColumnSchema {
        name: " ".to_string(),
        ..valid.clone()
    };
    assert!(invalid_name.validate().is_err());

    let invalid_type = ColumnSchema {
        type_descriptor: "\n".to_string(),
        ..valid.clone()
    };
    assert!(invalid_type.validate().is_err());
}

#[test]
fn test_mock_catalog_server_procedure_resolution() {
    let server = MockCatalogServer::new();
    let proc_id = ProcedureId::new(1);

    let result = server.resolve_procedure(proc_id);
    assert!(result.is_err());

    let manifest = ProcedureManifest {
        procedure_id: proc_id,
        ..valid_manifest()
    };

    assert!(server.register_procedure(manifest.clone()).is_ok());

    let resolved = server.resolve_procedure(proc_id).unwrap();
    assert_eq!(resolved.qualified_name, "public.my_proc");
    assert_eq!(resolved.procedure_id, proc_id);
}

#[test]
fn test_mock_catalog_server_is_not_durable_runtime() {
    let server = MockCatalogServer::new();

    let diagnostic = server.runtime_diagnostic();
    assert_eq!(diagnostic.kind, CatalogServerRuntimeKind::MockEphemeral);
    assert!(!diagnostic.is_durable());
    assert!(diagnostic.reason.contains("MockEphemeral"));
    assert!(diagnostic.actionable_message.contains("reopen"));

    let runtime_error = require_durable_catalog_runtime(&server).unwrap_err();
    assert_eq!(runtime_error.kind(), AndromedaErrorKind::Catalog);
    assert!(runtime_error.message().contains("MockEphemeral"));

    let handle_error = DurableCatalogRuntimeHandle::validate(&server).unwrap_err();
    assert_eq!(handle_error.kind(), AndromedaErrorKind::Catalog);
    assert!(handle_error.message().contains("MockEphemeral"));
}

#[test]
fn test_explicit_durable_runtime_evidence_is_accepted() {
    let evidence = CatalogRuntimeEvidence::durable(
        Some(CatalogVersion::new(7)),
        Some(3),
        CatalogRuntimeReopenEvidence::new(99, "unit-test-reopen"),
    );

    let handle = DurableCatalogRuntimeHandle::from_evidence(evidence).unwrap();
    assert_eq!(
        handle.evidence().catalog_version,
        Some(CatalogVersion::new(7))
    );
    assert_eq!(handle.evidence().epoch, Some(3));

    let server = DiagnosticOnlyCatalogServer {
        diagnostic: evidence.diagnostic(),
    };
    let diagnostic = require_durable_catalog_runtime(&server).unwrap();
    assert_eq!(diagnostic.kind, CatalogServerRuntimeKind::Durable);
    assert!(diagnostic.is_durable());
    assert!(diagnostic.reason.contains("validated"));
}

#[test]
fn test_missing_reopen_evidence_is_rejected_for_durable_runtime() {
    let evidence = CatalogRuntimeEvidence::durable_without_reopen_evidence(
        Some(CatalogVersion::new(8)),
        Some(4),
    );
    let diagnostic = evidence.diagnostic();

    assert_eq!(diagnostic.kind, CatalogServerRuntimeKind::Durable);
    assert_eq!(diagnostic.catalog_version, Some(CatalogVersion::new(8)));
    assert_eq!(diagnostic.epoch, Some(4));
    assert!(diagnostic.reopen_evidence.is_none());
    assert!(!diagnostic.is_durable());
    assert!(diagnostic.reason.contains("reopen evidence"));
    assert!(diagnostic.actionable_message.contains("attach"));

    let handle_error = DurableCatalogRuntimeHandle::from_evidence(evidence).unwrap_err();
    assert_eq!(handle_error.kind(), AndromedaErrorKind::Catalog);
    assert!(handle_error.message().contains("reopen evidence"));

    let server = DiagnosticOnlyCatalogServer { diagnostic };
    let runtime_error = require_durable_catalog_runtime(&server).unwrap_err();
    assert_eq!(runtime_error.kind(), AndromedaErrorKind::Catalog);
    assert!(
        runtime_error
            .message()
            .contains("attach validated reopen evidence")
    );
}

#[test]
fn test_mock_catalog_server_version_tracking() {
    let server = MockCatalogServer::new();

    let initial_version = server.get_catalog_version();
    assert_eq!(initial_version.get(), 1);

    assert!(server.advance_catalog_version(100).is_ok());
    let new_version = server.get_catalog_version();
    assert_eq!(new_version.get(), 2);

    assert!(server.advance_catalog_version(200).is_ok());
    let newer_version = server.get_catalog_version();
    assert_eq!(newer_version.get(), 3);
}

#[test]
fn test_mock_catalog_server_change_subscription() {
    let server = MockCatalogServer::new();

    assert!(server.advance_catalog_version(100).is_ok());
    assert!(server.advance_catalog_version(200).is_ok());
    assert!(server.advance_catalog_version(300).is_ok());

    let mut subscription = server.subscribe_to_changes().unwrap();
    assert!(subscription.is_active());

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

    let change4 = subscription.next_change();
    assert!(change4.is_none());

    subscription.close();
    assert!(!subscription.is_active());
}

#[test]
fn test_catalog_subscription_registry_requires_ordered_version_changes() {
    let registry = CatalogSubscriptionRegistry::new();
    let first = CatalogChangeNotification {
        new_version: CatalogVersion::new(2),
        previous_version: CatalogVersion::new(1),
        invalidation_boundary_lsn: 100,
    };

    registry.publish(first).unwrap();
    registry.publish(first).unwrap();

    let mut subscription = registry.subscribe().unwrap();
    assert_eq!(subscription.next_change(), Some(first));
    assert!(subscription.next_change().is_none());

    let out_of_order = CatalogChangeNotification {
        new_version: CatalogVersion::new(4),
        previous_version: CatalogVersion::new(2),
        invalidation_boundary_lsn: 200,
    };
    let error = registry.publish(out_of_order).unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("version ordering"));

    let zero_lsn = CatalogChangeNotification {
        new_version: CatalogVersion::new(3),
        previous_version: CatalogVersion::new(2),
        invalidation_boundary_lsn: 0,
    };
    let error = registry.publish(zero_lsn).unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    assert!(error.message().contains("boundary LSN"));
}

#[test]
fn test_catalog_change_notification_affects_procedure() {
    let notification = CatalogChangeNotification {
        new_version: CatalogVersion::new(2),
        previous_version: CatalogVersion::new(1),
        invalidation_boundary_lsn: 100,
    };

    assert!(notification.affects_procedure(ProcedureId::new(1)));
    assert!(notification.affects_procedure(ProcedureId::new(999)));

    let no_change = CatalogChangeNotification {
        new_version: CatalogVersion::new(1),
        previous_version: CatalogVersion::new(1),
        invalidation_boundary_lsn: 100,
    };
    assert!(!no_change.affects_procedure(ProcedureId::new(1)));
}
