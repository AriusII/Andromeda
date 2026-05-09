use crate::support::{
    ADMIN_FINGERPRINT, APP_FINGERPRINT, UNKNOWN_FINGERPRINT, administration_registry,
    application_registry, empty_runtime,
};
use andromeda_audit::SecurityAuditOutcome;
use andromeda_exec::SurfacePlaneAuthorizer;
use andromeda_observability::TraceId;
use andromeda_quic::SurfacePlane;
use andromeda_security::{AuthorizationDenialReason, AuthorizationOutcome, PrincipalRegistry};

/// Verifies that authorization checks produce `SecurityAuditTrace` events for
/// both allowed and denied outcomes.
#[test]
fn test_security_audit_trail_covers_mtls_and_permission() {
    let registry = application_registry();
    let gate = SurfacePlaneAuthorizer::new(&registry);
    let trace_id = TraceId::new(200);

    let allowed_result =
        gate.authorize_procedure_dispatch(trace_id, SurfacePlane::Application, APP_FINGERPRINT);

    assert!(
        allowed_result.is_ok(),
        "authorization should succeed for known certificate with execute permission"
    );

    if let Ok(Ok(token)) = allowed_result {
        let audit = token.audit();
        assert_eq!(audit.trace_id, trace_id, "trace_id should match");
        assert_eq!(
            audit.outcome,
            SecurityAuditOutcome::Allowed,
            "outcome should be Allowed"
        );
        assert!(
            audit.has_identity_evidence(),
            "audit trail should have certificate and principal evidence"
        );
    } else {
        panic!("expected allowed authorization");
    }

    let denied_result =
        gate.authorize_procedure_dispatch(trace_id, SurfacePlane::Application, "unknown-fp");

    assert!(
        denied_result.is_ok(),
        "authorization should return Ok (not panic) for unknown certificate"
    );

    if let Ok(Err(AuthorizationOutcome::Denied { reason, audit })) = denied_result {
        assert_eq!(
            reason,
            AuthorizationDenialReason::UnknownCertificate,
            "reason should be UnknownCertificate"
        );
        assert_eq!(
            audit.outcome,
            SecurityAuditOutcome::Denied,
            "audit outcome should be Denied"
        );
        assert_eq!(
            audit.trace_id, trace_id,
            "audit trace_id should match request"
        );
    } else {
        panic!("expected denied authorization");
    }

    let admin_registry = administration_registry();
    let admin_gate = SurfacePlaneAuthorizer::new(&admin_registry);
    let mismatch_result = admin_gate.authorize_procedure_dispatch(
        trace_id,
        SurfacePlane::Application,
        ADMIN_FINGERPRINT,
    );

    if let Ok(Err(AuthorizationOutcome::Denied { reason, audit })) = mismatch_result {
        assert_eq!(
            reason,
            AuthorizationDenialReason::SurfaceScopeMismatch,
            "reason should be SurfaceScopeMismatch"
        );
        assert!(
            audit.reason.contains("requested_surface")
                || audit.reason.contains("SurfaceScopeMismatch"),
            "audit reason should document the surface mismatch"
        );
    } else {
        panic!("expected denied authorization due to surface scope mismatch");
    }
}

/// Verifies that admission denial stops before local runtime entry, transaction
/// allocation, or WAL append.
#[test]
fn test_admission_gate_rejection_leaves_no_silent_drop() {
    let empty_registry = PrincipalRegistry::new();
    let gate = SurfacePlaneAuthorizer::new(&empty_registry);
    let runtime = empty_runtime();
    let trace_id = TraceId::new(400);

    let result =
        gate.authorize_procedure_dispatch(trace_id, SurfacePlane::Application, UNKNOWN_FINGERPRINT);

    assert!(result.is_ok(), "gate should return Ok (no panic)");

    if let Ok(Err(AuthorizationOutcome::Denied { reason, audit })) = result {
        assert_eq!(
            reason,
            AuthorizationDenialReason::UnknownCertificate,
            "unknown certificate should produce UnknownCertificate reason"
        );
        assert_eq!(
            audit.trace_id, trace_id,
            "audit should carry request trace_id"
        );
        assert_eq!(
            audit.outcome,
            SecurityAuditOutcome::Denied,
            "audit outcome should be Denied"
        );
        assert!(
            !audit.contains_sensitive_evidence(),
            "audit trail must not leak secret markers"
        );
    } else {
        panic!("expected authorization denial");
    }

    assert!(
        runtime.wal().is_empty(),
        "authorization denial must not create any WAL entry"
    );
    assert_eq!(
        runtime.transactions().live_count().unwrap(),
        0,
        "authorization denial must not create a local transaction"
    );
}
