use crate::support::*;

#[test]
fn durable_security_audit_records_require_binding_to_match_trace_and_correlation() {
    let envelope = security_audit_envelope(
        31,
        131,
        SurfaceScope::Application,
        Permission::ExecuteProcedure,
        SecurityAuditOutcome::Denied,
        "permission denial is durable before transaction creation",
    );

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
fn surface_scope_mismatch_denial_is_envelope_valid_and_durable_audit_ready() {
    let reason = format!(
        "denied:{}:cert_surface={:?}:requested_surface={:?}:action=admin_operation",
        andromeda_observe::SecurityAuditDenialReason::SurfaceScopeMismatch.label(),
        SurfaceScope::Application,
        SurfaceScope::Administration,
    );
    let envelope = EventEnvelope::new(
        EventId::new(33),
        request_correlation(),
        TraceEvent::SecurityAudit(
            SecurityAuditTrace::new(
                TraceId::new(133),
                SurfaceScope::Administration,
                certificate(SurfaceScope::Application),
                principal(),
                Permission::ManageSecurity,
                SecurityAuditOutcome::Denied,
                reason,
            )
            .expect("surface mismatch denial keeps typed IAM evidence"),
        ),
    )
    .expect("typed surface scope mismatch denial is envelope-valid");

    let mut binding = durable_security_binding();
    binding.surface = Some(SurfaceScope::Administration);
    binding.permission = Some(Permission::ManageSecurity);

    let record = PendingDurableAuditRecord::new(
        8,
        binding,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        envelope,
    )
    .expect("typed surface scope mismatch denial is durable-audit ready");

    assert_eq!(
        record.identity.family,
        DurableAuditEventFamily::SecurityDecision
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

    for (index, event) in events.into_iter().enumerate() {
        let envelope = EventEnvelope::new(
            EventId::new(150 + index as u128),
            request_correlation(),
            event,
        )
        .expect("typed protocol rejection should be envelope-valid admission evidence");
        let record = PendingDurableAuditRecord::new(
            50 + index as u64,
            durable_security_binding(),
            DurableAuditRetentionBoundary::ForensicHold,
            DurableAuditReplayBehavior::ForensicOnly,
            envelope,
        )
        .expect("typed protocol rejection should be durable-audit eligible");

        assert_eq!(
            record.identity.family,
            DurableAuditEventFamily::AdmissionDecision
        );
    }
}
