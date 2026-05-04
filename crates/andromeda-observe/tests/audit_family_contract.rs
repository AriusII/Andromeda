use andromeda_core::{RequestId, SessionId, TransactionId};
use andromeda_observe::{
    AdminOperation, AdminOperationTrace, CertificateIdentity, CriticalDecisionKind,
    EventCorrelation, EventEnvelope, EventId, EventSchemaVersion, Permission, PermissionFamily,
    SecurityAuditOutcome, SecurityAuditTrace, SurfaceScope, TraceEvent, TraceId, UserPrincipal,
    UserPrincipalKind, V0_EVENT_SCHEMA_VERSION,
};

fn request_correlation() -> EventCorrelation {
    EventCorrelation {
        request_id: Some(RequestId::new(70)),
        session_id: Some(SessionId::new(80)),
        contract_hash: None,
        catalog_version: None,
        catalog_object_id: None,
        transaction_id: None,
        durable_lsn: None,
        protocol: None,
    }
}

fn certificate(surface: SurfaceScope) -> CertificateIdentity {
    CertificateIdentity::new(
        "sha256:certificate-audit-test",
        "CN=andromeda-test",
        surface,
    )
    .expect("test certificate identity has explicit non-secret evidence")
}

fn principal() -> UserPrincipal {
    UserPrincipal::new("user:alice", UserPrincipalKind::Human)
        .expect("test principal has explicit identity evidence")
}

#[test]
fn security_audit_family_records_typed_identity_permission_schema_and_reason() {
    let trace = SecurityAuditTrace::new(
        TraceId::new(101),
        SurfaceScope::Application,
        certificate(SurfaceScope::Application),
        principal(),
        Permission::ExecuteProcedure,
        SecurityAuditOutcome::Allowed,
        "application principal was authorized before transaction creation",
    )
    .expect("security audit helper requires explicit reason evidence");

    let envelope = EventEnvelope::new(
        EventId::new(1),
        request_correlation(),
        TraceEvent::SecurityAudit(trace),
    )
    .expect("security audit event carries mandatory IAM evidence");

    assert_eq!(envelope.event.kind(), CriticalDecisionKind::SecurityAudit);
    match &envelope.event {
        TraceEvent::SecurityAudit(trace) => {
            assert_eq!(trace.schema_version, V0_EVENT_SCHEMA_VERSION);
            assert_eq!(trace.surface, SurfaceScope::Application);
            assert_eq!(trace.permission.family(), PermissionFamily::Application);
            assert_eq!(trace.outcome, SecurityAuditOutcome::Allowed);
            assert!(trace.has_identity_evidence());
            assert!(trace.has_reason());
        }
        _ => panic!("expected security audit trace"),
    }
}

#[test]
fn admin_operation_family_rejects_application_surface_and_permission_drift() {
    let admin_trace = AdminOperationTrace::new(
        TraceId::new(201),
        SurfaceScope::Administration,
        certificate(SurfaceScope::Administration),
        principal(),
        AdminOperation::DebugProcedure,
        Permission::DebugProcedure,
        true,
        "debug command used an isolated administration snapshot",
    )
    .expect("admin operation helper requires explicit reason evidence");

    let envelope = EventEnvelope::new(
        EventId::new(2),
        request_correlation(),
        TraceEvent::AdminOperation(admin_trace),
    )
    .expect("administration surface can carry a typed admin operation trace");

    assert_eq!(envelope.event.kind(), CriticalDecisionKind::AdminOperation);
    match &envelope.event {
        TraceEvent::AdminOperation(trace) => {
            assert!(trace.surface_permits_operation());
            assert!(trace.permission_matches_operation());
            assert_eq!(
                trace.operation.required_permission(),
                Permission::DebugProcedure
            );
        }
        _ => panic!("expected admin operation trace"),
    }

    let application_surface = EventEnvelope::new(
        EventId::new(3),
        request_correlation(),
        TraceEvent::AdminOperation(
            AdminOperationTrace::new(
                TraceId::new(202),
                SurfaceScope::Application,
                certificate(SurfaceScope::Application),
                principal(),
                AdminOperation::DebugProcedure,
                Permission::DebugProcedure,
                false,
                "application surface attempted to reach debug administration command",
            )
            .expect("trace construction only requires shape evidence"),
        ),
    )
    .unwrap_err();
    assert!(application_surface
        .message()
        .contains("application surface cannot carry admin operation"));

    let permission_drift = EventEnvelope::new(
        EventId::new(4),
        request_correlation(),
        TraceEvent::AdminOperation(
            AdminOperationTrace::new(
                TraceId::new(203),
                SurfaceScope::Administration,
                certificate(SurfaceScope::Administration),
                principal(),
                AdminOperation::ManageSecurity,
                Permission::ReadContract,
                true,
                "admin operation must not borrow an application permission",
            )
            .expect("trace construction only requires shape evidence"),
        ),
    )
    .unwrap_err();
    assert!(permission_drift
        .message()
        .contains("permission evidence matching"));
}

#[test]
fn audit_families_reject_missing_schema_identity_transaction_and_secret_evidence() {
    let unsupported_schema = EventEnvelope::new(
        EventId::new(5),
        request_correlation(),
        TraceEvent::SecurityAudit(SecurityAuditTrace {
            trace_id: TraceId::new(301),
            schema_version: EventSchemaVersion::new(0),
            surface: SurfaceScope::Application,
            certificate: certificate(SurfaceScope::Application),
            principal: principal(),
            permission: Permission::ExecuteProcedure,
            outcome: SecurityAuditOutcome::Allowed,
            reason: "schema version must be explicit".to_string(),
        }),
    )
    .unwrap_err();
    assert!(unsupported_schema.message().contains("schema version"));

    let missing_principal = EventEnvelope::new(
        EventId::new(6),
        request_correlation(),
        TraceEvent::SecurityAudit(SecurityAuditTrace {
            trace_id: TraceId::new(302),
            schema_version: V0_EVENT_SCHEMA_VERSION,
            surface: SurfaceScope::Application,
            certificate: certificate(SurfaceScope::Application),
            principal: UserPrincipal {
                principal_id: "   ".to_string(),
                kind: UserPrincipalKind::Human,
            },
            permission: Permission::ReadContract,
            outcome: SecurityAuditOutcome::Allowed,
            reason: "principal identity must be explicit".to_string(),
        }),
    )
    .unwrap_err();
    assert!(missing_principal.message().contains("principal identity"));

    let mut tx_correlation = request_correlation();
    tx_correlation.transaction_id = Some(TransactionId::new(99));
    let denied_with_transaction = EventEnvelope::new(
        EventId::new(7),
        tx_correlation,
        TraceEvent::SecurityAudit(
            SecurityAuditTrace::new(
                TraceId::new(303),
                SurfaceScope::Application,
                certificate(SurfaceScope::Application),
                principal(),
                Permission::ExecuteProcedure,
                SecurityAuditOutcome::Denied,
                "permission was denied before transaction creation",
            )
            .expect("denial trace has explicit IAM evidence"),
        ),
    )
    .unwrap_err();
    assert!(denied_with_transaction
        .message()
        .contains("must not include transaction"));

    let secret_text = EventEnvelope::new(
        EventId::new(8),
        request_correlation(),
        TraceEvent::AdminOperation(
            AdminOperationTrace::new(
                TraceId::new(304),
                SurfaceScope::Administration,
                certificate(SurfaceScope::Administration),
                principal(),
                AdminOperation::ManageSecurity,
                Permission::ManageSecurity,
                false,
                "token=must-not-enter-audit-ledger",
            )
            .expect("trace construction only requires a non-empty reason"),
        ),
    )
    .unwrap_err();
    assert!(secret_text.message().contains("must not include secrets"));
}
