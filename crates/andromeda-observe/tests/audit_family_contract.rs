use andromeda_audit::{
    CertificateIdentity, Permission, SecurityAuditOutcome, SecurityAuditTrace, SurfaceScope,
    UserPrincipal, UserPrincipalKind,
};
use andromeda_observability::{EventId, TraceId};
use andromeda_observe::{EventCorrelation, EventEnvelope, TraceEvent};
use andromeda_types::{RequestId, SessionId};

#[test]
fn observe_envelope_wraps_audit_owned_security_trace() {
    let certificate = CertificateIdentity::new(
        "sha256:observe-envelope-security-audit",
        "CN=observe-envelope",
        SurfaceScope::Application,
    )
    .expect("test certificate has explicit evidence");
    let principal = UserPrincipal::new("user:observe-envelope", UserPrincipalKind::Human)
        .expect("test principal has explicit evidence");
    let trace = SecurityAuditTrace::new(
        TraceId::new(101),
        SurfaceScope::Application,
        certificate,
        principal,
        Permission::ExecuteProcedure,
        SecurityAuditOutcome::Allowed,
        "observe envelope carries audit-owned security trace",
    )
    .expect("audit-owned trace construction succeeds");

    let envelope = EventEnvelope::new(
        EventId::new(1),
        EventCorrelation {
            request_id: Some(RequestId::new(70)),
            session_id: Some(SessionId::new(80)),
            contract_hash: None,
            catalog_version: None,
            catalog_object_id: None,
            transaction_id: None,
            durable_lsn: None,
            protocol: None,
        },
        TraceEvent::SecurityAudit(trace),
    )
    .expect("observe runtime accepts audit-owned security evidence");

    assert!(matches!(envelope.event, TraceEvent::SecurityAudit(_)));
}
