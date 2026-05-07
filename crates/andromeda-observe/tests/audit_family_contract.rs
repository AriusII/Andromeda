use andromeda_core::{RequestId, SessionId, TransactionId};
use andromeda_observe::{
    AdminOperation, AdminOperationTrace, AuthorizationDeniedTrace, CertificateIdentity,
    CriticalDecisionKind, DurableAuditEventFamily, DurableAuditPrincipalBinding,
    DurableAuditReplayBehavior, DurableAuditRetentionBoundary, EventCorrelation, EventEnvelope,
    EventId, EventSchemaVersion, FrameRejectionTrace, PendingDurableAuditRecord, Permission,
    PermissionFamily, ProtocolCorrelation, ProtocolEventScope, SchemaLayoutDecisionTrace,
    SecurityAuditOutcome, SecurityAuditTrace, SecurityPolicyVersionEvidence,
    StreamRoleRejectionTrace, SurfaceScope, TraceEvent, TraceId, UnsupportedVersionTrace,
    UserPrincipal, UserPrincipalKind, V0_EVENT_SCHEMA_VERSION, durable_audit_family,
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

fn protocol_correlation() -> ProtocolCorrelation {
    ProtocolCorrelation {
        protocol_version: Some(1),
        stream_id: Some(30),
        stream_role: Some(2),
        frame_type: Some(40),
        payload_kind: Some(50),
        sequence: Some(60),
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

fn durable_security_binding() -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: "user:alice".to_string(),
        certificate_fingerprint: Some("sha256:certificate-audit-test".to_string()),
        surface: Some(SurfaceScope::Application),
        permission: Some(Permission::ExecuteProcedure),
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
        request_id: Some(RequestId::new(70)),
        session_id: Some(SessionId::new(80)),
    }
}

fn alternate_policy_version() -> SecurityPolicyVersionEvidence {
    SecurityPolicyVersionEvidence::new(
        2,
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    )
    .expect("alternate test policy version is canonical")
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
            assert_eq!(
                trace.policy_version,
                SecurityPolicyVersionEvidence::bootstrap_v0()
            );
            assert!(trace.has_identity_evidence());
            assert!(trace.has_policy_version_evidence());
            assert!(trace.has_reason());
        }
        _ => panic!("expected security audit trace"),
    }
}

#[test]
fn security_audit_policy_bootstrap_is_available_as_typed_evidence() {
    let evidence = SecurityPolicyVersionEvidence::try_bootstrap_v0()
        .expect("current security policy evidence is typed and canonical");

    assert!(evidence.has_version_evidence());
    assert_eq!(SecurityPolicyVersionEvidence::bootstrap_v0(), evidence);
}

#[test]
fn security_audit_rejects_broader_secret_markers() {
    let err = EventEnvelope::new(
        EventId::new(21),
        request_correlation(),
        TraceEvent::SecurityAudit(
            SecurityAuditTrace::new(
                TraceId::new(121),
                SurfaceScope::Application,
                certificate(SurfaceScope::Application),
                principal(),
                Permission::ExecuteProcedure,
                SecurityAuditOutcome::Denied,
                "bearer raw-session-token must not enter audit",
            )
            .expect("trace construction validates shape before envelope secret safety"),
        ),
    )
    .unwrap_err();
    assert!(err.message().contains("must not include secrets"));

    let err = EventEnvelope::new(
        EventId::new(22),
        request_correlation(),
        TraceEvent::SecurityAudit(
            SecurityAuditTrace::new(
                TraceId::new(122),
                SurfaceScope::Application,
                CertificateIdentity::new(
                    "x-api-key=must-not-enter-audit",
                    "CN=andromeda-test",
                    SurfaceScope::Application,
                )
                .expect("certificate construction validates non-empty evidence"),
                principal(),
                Permission::ExecuteProcedure,
                SecurityAuditOutcome::Denied,
                "certificate evidence must be secret-safe",
            )
            .expect("trace construction validates shape before envelope secret safety"),
        ),
    )
    .unwrap_err();
    assert!(err.message().contains("must not include secrets"));
}

#[test]
fn durable_security_audit_records_require_binding_to_match_trace_and_correlation() {
    let envelope = EventEnvelope::new(
        EventId::new(31),
        request_correlation(),
        TraceEvent::SecurityAudit(
            SecurityAuditTrace::new(
                TraceId::new(131),
                SurfaceScope::Application,
                certificate(SurfaceScope::Application),
                principal(),
                Permission::ExecuteProcedure,
                SecurityAuditOutcome::Denied,
                "permission denial is durable before transaction creation",
            )
            .expect("security audit trace has typed denial evidence"),
        ),
    )
    .expect("security audit envelope is request/session correlated");

    PendingDurableAuditRecord::new(
        1,
        durable_security_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        envelope.clone(),
    )
    .expect("matching security audit binding is durable-audit eligible");

    let mut missing_permission = durable_security_binding();
    missing_permission.permission = None;
    let missing_permission_err = PendingDurableAuditRecord::new(
        2,
        missing_permission,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        envelope.clone(),
    )
    .unwrap_err();
    assert!(
        missing_permission_err
            .message()
            .contains("certificate, surface, permission, and policy version evidence")
    );

    let mut missing_policy_version = durable_security_binding();
    missing_policy_version.policy_version = None;
    let missing_policy_version_err = PendingDurableAuditRecord::new(
        6,
        missing_policy_version,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        envelope.clone(),
    )
    .unwrap_err();
    assert!(
        missing_policy_version_err
            .message()
            .contains("policy version evidence")
    );

    let mut wrong_principal = durable_security_binding();
    wrong_principal.principal_id = "user:bob".to_string();
    let wrong_principal_err = PendingDurableAuditRecord::new(
        3,
        wrong_principal,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        envelope.clone(),
    )
    .unwrap_err();
    assert!(
        wrong_principal_err
            .message()
            .contains("principal_id must match")
    );

    let mut wrong_correlation = durable_security_binding();
    wrong_correlation.request_id = Some(RequestId::new(71));
    let wrong_correlation_err = PendingDurableAuditRecord::new(
        4,
        wrong_correlation,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        envelope.clone(),
    )
    .unwrap_err();
    assert!(
        wrong_correlation_err
            .message()
            .contains("request/session ids must match")
    );

    let mut wrong_policy_version = durable_security_binding();
    wrong_policy_version.policy_version = Some(alternate_policy_version());
    let wrong_policy_version_err = PendingDurableAuditRecord::new(
        7,
        wrong_policy_version,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        envelope,
    )
    .unwrap_err();
    assert!(
        wrong_policy_version_err
            .message()
            .contains("policy version evidence must match")
    );
}

#[test]
fn legacy_authorization_denied_is_not_durable_security_decision_evidence() {
    let legacy_denial = EventEnvelope::new(
        EventId::new(32),
        request_correlation(),
        TraceEvent::AuthorizationDenied(AuthorizationDeniedTrace {
            trace_id: TraceId::new(132),
            denied_permission: "Inventory.ReserveStock.Execute".to_string(),
            reason:
                "legacy denial lacks typed certificate, principal, surface, and policy evidence"
                    .to_string(),
        }),
    )
    .expect("legacy authorization denial remains observable");

    let err = PendingDurableAuditRecord::new(
        5,
        durable_security_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        legacy_denial,
    )
    .unwrap_err();

    assert!(err.message().contains("not eligible"));
}

#[test]
fn typed_protocol_rejections_are_durable_admission_decisions() {
    let events = [
        TraceEvent::FrameRejection(FrameRejectionTrace {
            trace_id: TraceId::new(151),
            scope: ProtocolEventScope::Request,
            protocol: protocol_correlation(),
            reason: "frame length exceeds request limit".to_string(),
        }),
        TraceEvent::StreamRoleRejection(StreamRoleRejectionTrace {
            trace_id: TraceId::new(152),
            scope: ProtocolEventScope::Request,
            stream_id: Some(30),
            observed_role: Some(3),
            expected_role: Some(2),
            reason: "stream role does not match request frame".to_string(),
        }),
        TraceEvent::UnsupportedVersion(UnsupportedVersionTrace {
            trace_id: TraceId::new(153),
            scope: ProtocolEventScope::Connection,
            offered_version: Some(99),
            min_supported_version: Some(1),
            max_supported_version: Some(2),
            reason: "offered protocol version is outside supported range".to_string(),
        }),
        TraceEvent::SchemaLayoutDecision(SchemaLayoutDecisionTrace {
            trace_id: TraceId::new(154),
            scope: ProtocolEventScope::Request,
            schema_id: Some(11),
            schema_version: Some(12),
            layout_id: Some(13),
            layout_version: Some(14),
            accepted: false,
            reason: "schema and layout versions do not match request contract".to_string(),
        }),
    ];

    for event in events {
        assert_eq!(
            durable_audit_family(&event),
            Some(DurableAuditEventFamily::AdmissionDecision)
        );
    }
}

#[test]
fn security_audit_rejects_application_definition_permission_drift() {
    for (event_id, trace_id, permission) in [
        (41, 141, Permission::CreateTable),
        (42, 142, Permission::ImportDefinitionBatch),
    ] {
        let err = EventEnvelope::new(
            EventId::new(event_id),
            request_correlation(),
            TraceEvent::SecurityAudit(
                SecurityAuditTrace::new(
                    TraceId::new(trace_id),
                    SurfaceScope::Application,
                    certificate(SurfaceScope::Application),
                    principal(),
                    permission,
                    SecurityAuditOutcome::Denied,
                    "application surface must not carry definition permission evidence",
                )
                .expect("trace construction only validates shape evidence"),
            ),
        )
        .unwrap_err();

        assert!(err.message().contains("surface scope"));
    }
}

#[test]
fn security_audit_rejects_admin_cluster_surface_permission_drift() {
    for (event_id, trace_id, surface, permission) in [
        (
            43,
            143,
            SurfaceScope::Administration,
            Permission::ClusterPromote,
        ),
        (44, 144, SurfaceScope::Cluster, Permission::ManageSecurity),
    ] {
        let err = EventEnvelope::new(
            EventId::new(event_id),
            request_correlation(),
            TraceEvent::SecurityAudit(
                SecurityAuditTrace::new(
                    TraceId::new(trace_id),
                    surface,
                    certificate(surface),
                    principal(),
                    permission,
                    SecurityAuditOutcome::Denied,
                    "surface must carry only its own permission family",
                )
                .expect("trace construction only validates shape evidence"),
            ),
        )
        .unwrap_err();

        assert!(err.message().contains("surface scope"));
    }
}

#[test]
fn security_audit_rejects_invalid_policy_version_evidence() {
    let err = EventEnvelope::new(
        EventId::new(45),
        request_correlation(),
        TraceEvent::SecurityAudit(SecurityAuditTrace {
            trace_id: TraceId::new(145),
            schema_version: V0_EVENT_SCHEMA_VERSION,
            surface: SurfaceScope::Application,
            certificate: certificate(SurfaceScope::Application),
            principal: principal(),
            permission: Permission::ExecuteProcedure,
            outcome: SecurityAuditOutcome::Denied,
            policy_version: SecurityPolicyVersionEvidence {
                policy_version: 0,
                policy_digest:
                    "sha256:2222222222222222222222222222222222222222222222222222222222222222"
                        .to_string(),
            },
            reason: "policy version evidence must be non-zero and typed".to_string(),
        }),
    )
    .unwrap_err();

    assert!(err.message().contains("policy version evidence"));
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
    assert!(
        application_surface
            .message()
            .contains("surface cannot carry admin operation")
    );

    let cluster_operation_on_admin_surface = EventEnvelope::new(
        EventId::new(30),
        request_correlation(),
        TraceEvent::AdminOperation(
            AdminOperationTrace::new(
                TraceId::new(230),
                SurfaceScope::Administration,
                certificate(SurfaceScope::Administration),
                principal(),
                AdminOperation::ClusterPromote,
                Permission::ClusterPromote,
                false,
                "administration surface attempted to carry cluster promotion",
            )
            .expect("trace construction only requires shape evidence"),
        ),
    )
    .unwrap_err();
    assert!(
        cluster_operation_on_admin_surface
            .message()
            .contains("surface cannot carry admin operation")
    );

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
    assert!(
        permission_drift
            .message()
            .contains("permission evidence matching")
    );
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
            policy_version: SecurityPolicyVersionEvidence::bootstrap_v0(),
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
            policy_version: SecurityPolicyVersionEvidence::bootstrap_v0(),
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
    assert!(
        denied_with_transaction
            .message()
            .contains("must not include transaction")
    );

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
