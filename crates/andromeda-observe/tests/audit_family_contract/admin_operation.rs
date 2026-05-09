use crate::support::*;

#[test]
fn admin_operation_family_rejects_application_surface_and_permission_drift() {
    let admin_trace = admin_operation_trace(
        201,
        SurfaceScope::Administration,
        AdminOperation::DebugProcedure,
        Permission::DebugProcedure,
        true,
        "debug command used an isolated administration snapshot",
    );

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
        },
        _ => panic!("expected admin operation trace"),
    }

    let application_surface = EventEnvelope::new(
        EventId::new(3),
        request_correlation(),
        TraceEvent::AdminOperation(admin_operation_trace(
            202,
            SurfaceScope::Application,
            AdminOperation::DebugProcedure,
            Permission::DebugProcedure,
            false,
            "application surface attempted to reach debug administration command",
        )),
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
        TraceEvent::AdminOperation(admin_operation_trace(
            230,
            SurfaceScope::Administration,
            AdminOperation::ClusterPromote,
            Permission::ClusterPromote,
            false,
            "administration surface attempted to carry cluster promotion",
        )),
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
        TraceEvent::AdminOperation(admin_operation_trace(
            203,
            SurfaceScope::Administration,
            AdminOperation::ManageSecurity,
            Permission::ReadContract,
            true,
            "admin operation must not borrow an application permission",
        )),
    )
    .unwrap_err();
    assert!(
        permission_drift
            .message()
            .contains("permission evidence matching")
    );
}

#[test]
fn durable_admin_operation_families_require_binding_to_match_operation_trace() {
    let cases = [
        (
            DurableAuditEventFamily::AdminDecision,
            SurfaceScope::Administration,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
        (
            DurableAuditEventFamily::HadrDecision,
            SurfaceScope::Cluster,
            AdminOperation::ClusterPromote,
            Permission::ClusterPromote,
        ),
        (
            DurableAuditEventFamily::BackupDecision,
            SurfaceScope::BackupAgent,
            AdminOperation::Backup,
            Permission::Backup,
        ),
        (
            DurableAuditEventFamily::RestoreDecision,
            SurfaceScope::BackupAgent,
            AdminOperation::Restore,
            Permission::Restore,
        ),
        (
            DurableAuditEventFamily::ForensicDecision,
            SurfaceScope::BackupAgent,
            AdminOperation::ForensicStart,
            Permission::ForensicStart,
        ),
    ];

    for (index, (expected_family, surface, operation, permission)) in cases.into_iter().enumerate()
    {
        let event_id = 60 + index as u128;
        let envelope = admin_operation_envelope(
            event_id,
            260 + index as u128,
            surface,
            operation,
            permission,
        );
        let record = PendingDurableAuditRecord::new(
            60 + index as u64,
            durable_admin_operation_binding(surface, permission),
            DurableAuditRetentionBoundary::SecurityPolicy,
            DurableAuditReplayBehavior::RebuildDecisionIndex,
            envelope.clone(),
        )
        .expect("matching admin operation binding is durable-audit eligible");
        assert_eq!(record.identity.family, expected_family);

        let mut wrong_principal = durable_admin_operation_binding(surface, permission);
        wrong_principal.principal_id = "user:binding-drift".to_string();
        let principal_drift = PendingDurableAuditRecord::new(
            70 + index as u64,
            wrong_principal,
            DurableAuditRetentionBoundary::SecurityPolicy,
            DurableAuditReplayBehavior::RebuildDecisionIndex,
            envelope.clone(),
        )
        .unwrap_err();
        assert!(
            principal_drift
                .message()
                .contains("principal_id must match admin operation trace")
        );

        let mut missing_policy = durable_admin_operation_binding(surface, permission);
        missing_policy.policy_version = None;
        let missing_policy = PendingDurableAuditRecord::new(
            80 + index as u64,
            missing_policy,
            DurableAuditRetentionBoundary::SecurityPolicy,
            DurableAuditReplayBehavior::RebuildDecisionIndex,
            envelope,
        )
        .unwrap_err();
        assert!(missing_policy.message().contains("policy version evidence"));
    }
}

#[test]
fn durable_admin_operation_records_reject_surface_permission_certificate_and_correlation_drift() {
    let envelope = admin_operation_envelope(
        90,
        290,
        SurfaceScope::Administration,
        AdminOperation::InspectPlans,
        Permission::InspectPlans,
    );

    let mut wrong_surface =
        durable_admin_operation_binding(SurfaceScope::MonitoringAgent, Permission::InspectPlans);
    let surface_drift = PendingDurableAuditRecord::new(
        90,
        wrong_surface.clone(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        envelope.clone(),
    )
    .unwrap_err();
    assert!(
        surface_drift
            .message()
            .contains("surface must match admin operation trace")
    );

    let wrong_permission =
        durable_admin_operation_binding(SurfaceScope::Administration, Permission::DebugProcedure);
    let permission_drift = PendingDurableAuditRecord::new(
        91,
        wrong_permission,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        envelope.clone(),
    )
    .unwrap_err();
    assert!(
        permission_drift
            .message()
            .contains("permission must match admin operation trace")
    );

    wrong_surface.surface = Some(SurfaceScope::Administration);
    wrong_surface.certificate_fingerprint = Some("sha256:other-admin-certificate".to_string());
    let certificate_drift = PendingDurableAuditRecord::new(
        92,
        wrong_surface.clone(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        envelope.clone(),
    )
    .unwrap_err();
    assert!(
        certificate_drift
            .message()
            .contains("certificate fingerprint must match admin operation trace")
    );

    wrong_surface.certificate_fingerprint = Some("sha256:certificate-audit-test".to_string());
    wrong_surface.request_id = Some(RequestId::new(71));
    let correlation_drift = PendingDurableAuditRecord::new(
        93,
        wrong_surface.clone(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        envelope.clone(),
    )
    .unwrap_err();
    assert!(
        correlation_drift
            .message()
            .contains("request/session ids must match envelope correlation")
    );

    wrong_surface.request_id = Some(RequestId::new(70));
    wrong_surface.certificate_fingerprint = None;
    let missing_certificate = PendingDurableAuditRecord::new(
        94,
        wrong_surface,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        envelope,
    )
    .unwrap_err();
    assert!(
        missing_certificate
            .message()
            .contains("certificate, surface, and permission evidence")
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
        TraceEvent::AdminOperation(admin_operation_trace(
            304,
            SurfaceScope::Administration,
            AdminOperation::ManageSecurity,
            Permission::ManageSecurity,
            false,
            "token=must-not-enter-audit-ledger",
        )),
    )
    .unwrap_err();
    assert!(secret_text.message().contains("must not include secrets"));
}
