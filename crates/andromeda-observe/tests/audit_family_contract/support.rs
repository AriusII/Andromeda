pub(crate) use andromeda_core::{RequestId, SessionId, TransactionId};
pub(crate) use andromeda_observe::{
    AdminOperation, AdminOperationTrace, AuthorizationDeniedTrace, CertificateIdentity,
    CriticalDecisionKind, DurableAuditEventFamily, DurableAuditPrincipalBinding,
    DurableAuditReplayBehavior, DurableAuditRetentionBoundary, EventCorrelation, EventEnvelope,
    EventId, EventSchemaVersion, FrameRejectionTrace, PendingDurableAuditRecord, Permission,
    PermissionFamily, ProtocolCorrelation, ProtocolEventScope, SchemaLayoutDecisionTrace,
    SecurityAuditOutcome, SecurityAuditTrace, SecurityPolicyVersionEvidence,
    StreamRoleRejectionTrace, SurfaceScope, TraceEvent, TraceId, UnsupportedVersionTrace,
    UserPrincipal, UserPrincipalKind, V0_EVENT_SCHEMA_VERSION,
};

pub(crate) fn request_correlation() -> EventCorrelation {
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

pub(crate) fn protocol_correlation() -> ProtocolCorrelation {
    ProtocolCorrelation {
        protocol_version: Some(1),
        stream_id: Some(30),
        stream_role: Some(2),
        frame_type: Some(40),
        payload_kind: Some(50),
        sequence: Some(60),
    }
}

pub(crate) fn certificate(surface: SurfaceScope) -> CertificateIdentity {
    CertificateIdentity::new(
        "sha256:certificate-audit-test",
        "CN=andromeda-test",
        surface,
    )
    .expect("test certificate identity has explicit non-secret evidence")
}

pub(crate) fn principal() -> UserPrincipal {
    UserPrincipal::new("user:alice", UserPrincipalKind::Human)
        .expect("test principal has explicit identity evidence")
}

pub(crate) fn durable_security_binding() -> DurableAuditPrincipalBinding {
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

pub(crate) fn alternate_policy_version() -> SecurityPolicyVersionEvidence {
    SecurityPolicyVersionEvidence::new(
        2,
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    )
    .expect("alternate test policy version is canonical")
}

pub(crate) fn security_audit_trace(
    trace_id: u128,
    surface: SurfaceScope,
    permission: Permission,
    outcome: SecurityAuditOutcome,
    reason: &str,
) -> SecurityAuditTrace {
    SecurityAuditTrace::new(
        TraceId::new(trace_id),
        surface,
        certificate(surface),
        principal(),
        permission,
        outcome,
        reason,
    )
    .expect("security audit trace has explicit IAM evidence")
}

pub(crate) fn security_audit_envelope(
    event_id: u128,
    trace_id: u128,
    surface: SurfaceScope,
    permission: Permission,
    outcome: SecurityAuditOutcome,
    reason: &str,
) -> EventEnvelope {
    EventEnvelope::new(
        EventId::new(event_id),
        request_correlation(),
        TraceEvent::SecurityAudit(security_audit_trace(
            trace_id, surface, permission, outcome, reason,
        )),
    )
    .expect("security audit event carries mandatory IAM evidence")
}

pub(crate) fn admin_operation_trace(
    trace_id: u128,
    surface: SurfaceScope,
    operation: AdminOperation,
    permission: Permission,
    succeeded: bool,
    reason: &str,
) -> AdminOperationTrace {
    AdminOperationTrace::new(
        TraceId::new(trace_id),
        surface,
        certificate(surface),
        principal(),
        operation,
        permission,
        succeeded,
        reason,
    )
    .expect("admin operation trace has explicit operation evidence")
}
