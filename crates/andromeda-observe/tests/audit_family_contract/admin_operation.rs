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
        }
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
