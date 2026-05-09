use andromeda_audit::{
    AdminOperation, AdminOperationTrace, CertificateIdentity, DurableAuditAppendRecord,
    DurableAuditEventFamily, DurableAuditPrincipalBinding, DurableAuditRecordIdentity,
    DurableAuditReplayBehavior, DurableAuditRetentionBoundary, Permission, PermissionFamily,
    SecurityAuditDenialReason, SecurityAuditOutcome, SecurityAuditTrace,
    SecurityPolicyVersionEvidence, SurfaceScope, UserPrincipal, UserPrincipalKind,
};
use andromeda_observability::{EventId, TraceId, V0_EVENT_SCHEMA_VERSION};
use andromeda_types::{RequestId, SessionId};

#[test]
fn security_audit_owner_records_typed_identity_permission_schema_and_reason() {
    let trace = security_audit_trace(
        101,
        SurfaceScope::Application,
        Permission::ExecuteProcedure,
        SecurityAuditOutcome::Allowed,
        "application principal was authorized before transaction creation",
    );

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
    assert!(trace.surface_permits_permission());
}

#[test]
fn security_audit_denial_reason_is_contract_owned_and_audit_parseable() {
    let reason = SecurityAuditDenialReason::SurfaceScopeMismatch;
    let text = format!(
        "denied:{}:cert_surface={:?}:requested_surface={:?}",
        reason.label(),
        SurfaceScope::Application,
        SurfaceScope::Administration,
    );
    let trace = SecurityAuditTrace::new(
        TraceId::new(133),
        SurfaceScope::Administration,
        certificate(SurfaceScope::Application),
        principal(),
        Permission::ManageSecurity,
        SecurityAuditOutcome::Denied,
        text,
    )
    .expect("surface mismatch denial keeps typed evidence");

    assert_eq!(trace.denial_reason(), Some(reason));
    assert!(trace.has_typed_surface_scope_mismatch_denial());
}

#[test]
fn security_audit_owner_rejects_invalid_policy_and_secret_evidence() {
    let invalid_policy = SecurityPolicyVersionEvidence {
        policy_version: 0,
        policy_digest: "sha256:2222222222222222222222222222222222222222222222222222222222222222"
            .to_string(),
    };
    let err = SecurityAuditTrace::new_with_policy_version(
        TraceId::new(145),
        SurfaceScope::Application,
        certificate(SurfaceScope::Application),
        principal(),
        Permission::ExecuteProcedure,
        SecurityAuditOutcome::Denied,
        invalid_policy,
        "policy version evidence must be non-zero and typed",
    )
    .expect_err("audit owner rejects invalid policy version evidence");
    assert!(err.message().contains("policy version evidence"));

    let trace = SecurityAuditTrace::new(
        TraceId::new(146),
        SurfaceScope::Application,
        certificate(SurfaceScope::Application),
        principal(),
        Permission::ExecuteProcedure,
        SecurityAuditOutcome::Denied,
        "bearer raw-session-token must not enter audit",
    )
    .expect("trace shape is valid before secret safety is checked");
    assert!(trace.contains_sensitive_evidence());
}

#[test]
fn admin_operation_owner_records_surface_permission_and_secret_invariants() {
    let trace = AdminOperationTrace::new(
        TraceId::new(201),
        SurfaceScope::Administration,
        certificate(SurfaceScope::Administration),
        principal(),
        AdminOperation::DebugProcedure,
        Permission::DebugProcedure,
        true,
        "debug command used an isolated administration snapshot",
    )
    .expect("admin operation trace has explicit operation evidence");

    assert!(trace.has_supported_schema_version());
    assert!(trace.has_identity_evidence());
    assert!(trace.has_reason());
    assert!(trace.surface_matches_certificate());
    assert!(trace.surface_permits_operation());
    assert!(trace.permission_matches_operation());
    assert_eq!(
        trace.operation.required_permission(),
        Permission::DebugProcedure
    );

    let secret_trace = AdminOperationTrace::new(
        TraceId::new(202),
        SurfaceScope::Administration,
        certificate(SurfaceScope::Administration),
        principal(),
        AdminOperation::ManageSecurity,
        Permission::ManageSecurity,
        false,
        "token=must-not-enter-audit-ledger",
    )
    .expect("trace shape is valid before secret safety is checked");
    assert!(secret_trace.contains_sensitive_evidence());
}

#[test]
fn durable_audit_owner_append_records_validate_critical_permission_binding() {
    let record = durable_append_record(
        DurableAuditEventFamily::SecurityDecision,
        SurfaceScope::Application,
        Permission::ExecuteProcedure,
    );
    record
        .validate()
        .expect("complete security decision evidence is audit-owned durable evidence");

    let mut drift = durable_append_record(
        DurableAuditEventFamily::AdminDecision,
        SurfaceScope::Administration,
        Permission::InspectPlans,
    );
    drift.principal_binding.permission = Some(Permission::ReadContract);
    let err = drift
        .validate()
        .expect_err("audit owner rejects surface/permission binding drift");
    assert!(
        err.message()
            .contains("surface must permit permission evidence")
    );
}

fn certificate(surface: SurfaceScope) -> CertificateIdentity {
    CertificateIdentity::new(
        "sha256:certificate-audit-owner-test",
        "CN=andromeda-test",
        surface,
    )
    .expect("test certificate identity has explicit non-secret evidence")
}

fn principal() -> UserPrincipal {
    UserPrincipal::new("user:alice", UserPrincipalKind::Human)
        .expect("test principal has explicit identity evidence")
}

fn security_audit_trace(
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

fn durable_append_record(
    family: DurableAuditEventFamily,
    surface: SurfaceScope,
    permission: Permission,
) -> DurableAuditAppendRecord {
    DurableAuditAppendRecord::new(
        DurableAuditRecordIdentity {
            event_id: EventId::new(10),
            trace_id: TraceId::new(90),
            family,
            sequence_number: 1,
        },
        DurableAuditPrincipalBinding {
            principal_id: "user:alice".to_string(),
            certificate_fingerprint: Some("sha256:certificate-audit-owner-test".to_string()),
            surface: Some(surface),
            permission: Some(permission),
            policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
            request_id: Some(RequestId::new(70)),
            session_id: Some(SessionId::new(80)),
        },
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        format!("{family:?}"),
    )
    .expect("sample durable append record is valid")
}
