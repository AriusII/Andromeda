use crate::support::*;

#[test]
fn security_audit_family_records_typed_identity_permission_schema_and_reason() {
    let envelope = security_audit_envelope(
        1,
        101,
        SurfaceScope::Application,
        Permission::ExecuteProcedure,
        SecurityAuditOutcome::Allowed,
        "application principal was authorized before transaction creation",
    );

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
fn security_audit_rejects_application_definition_permission_drift() {
    for (event_id, trace_id, permission) in [
        (41, 141, Permission::CreateTable),
        (42, 142, Permission::ImportDefinitionBatch),
    ] {
        let err = EventEnvelope::new(
            EventId::new(event_id),
            request_correlation(),
            TraceEvent::SecurityAudit(security_audit_trace(
                trace_id,
                SurfaceScope::Application,
                permission,
                SecurityAuditOutcome::Denied,
                "application surface must not carry definition permission evidence",
            )),
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
            TraceEvent::SecurityAudit(security_audit_trace(
                trace_id,
                surface,
                permission,
                SecurityAuditOutcome::Denied,
                "surface must carry only its own permission family",
            )),
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
