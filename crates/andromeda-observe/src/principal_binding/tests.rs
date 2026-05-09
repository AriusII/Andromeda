use super::*;
use crate::TraceId;
use crate::events::{
    CertificateIdentity, EventCorrelation, EventEnvelope, EventId, Permission,
    SecurityAuditOutcome, SurfaceScope, TraceEvent, UserPrincipal, UserPrincipalKind,
};
use andromeda_types::{RequestId, SessionId};

fn cert(fingerprint: &str, surface: SurfaceScope) -> CertificateIdentity {
    CertificateIdentity::new(fingerprint, format!("CN={}", fingerprint), surface).unwrap()
}

fn principal(id: &str) -> UserPrincipal {
    UserPrincipal::new(id, UserPrincipalKind::Service).unwrap()
}

fn binding(
    fingerprint: &str,
    surface: SurfaceScope,
    principal_id: &str,
    permissions: Vec<Permission>,
) -> PrincipalBinding {
    PrincipalBinding::new(
        cert(fingerprint, surface),
        principal(principal_id),
        permissions,
    )
    .unwrap()
}

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

#[test]
fn compatibility_reexport_authorizes_with_security_owner() {
    let mut registry = PrincipalRegistry::new();
    registry
        .register(binding(
            "fp-app-compat",
            SurfaceScope::Application,
            "svc-compat",
            vec![Permission::ExecuteProcedure],
        ))
        .unwrap();
    let auth = SurfaceAuthorizer::new(&registry);

    let outcome = auth
        .authorize(
            TraceId::new(5),
            SurfaceScope::Application,
            "fp-app-compat",
            SurfaceAction::ExecuteProcedure,
        )
        .unwrap();

    assert!(outcome.is_allowed());
    assert_eq!(outcome.audit().outcome, SecurityAuditOutcome::Allowed);
}

#[test]
fn compatibility_reexport_audit_payload_can_still_be_enveloped_by_observe() {
    let mut registry = PrincipalRegistry::new();
    registry
        .register(binding(
            "fp-app-compat-deny",
            SurfaceScope::Application,
            "svc-compat-deny",
            vec![Permission::ReadContract],
        ))
        .unwrap();
    let auth = SurfaceAuthorizer::new(&registry);

    let outcome = auth
        .authorize(
            TraceId::new(22),
            SurfaceScope::Application,
            "fp-app-compat-deny",
            SurfaceAction::ExecuteProcedure,
        )
        .unwrap();

    assert!(outcome.is_denied());
    let audit = outcome.audit().clone();
    assert_eq!(
        audit.denial_reason(),
        Some(AuthorizationDenialReason::PrincipalMissingPermission)
    );
    EventEnvelope::new(
        EventId::new(22),
        request_correlation(),
        TraceEvent::SecurityAudit(audit),
    )
    .expect("security-owned denial audit should remain envelope-valid through observe");
}
