//! Runtime-free Quinn admission contract for reconnect/pool/0-RTT wiring.

#![cfg(feature = "runtime-quinn")]

use andromeda_core::{AndromedaErrorKind, CertificateIdentityStatus};
use andromeda_observe::{CertificateIdentity, SurfaceScope};
use andromeda_quic::{
    ConnectionPoolKey, PoolAdmissionKind, ReconnectState, RetryAdmissionDecision, RetryIdempotency,
    RetryRejectionReason, SurfacePlane, ZeroRttAdmissionRejectionReason, ZeroRttReplayClass,
    quinn_backend::{
        QuinnAdmissionPlanner, QuinnAdmissionRequest, QuinnCertificateStatusEvidence,
        QuinnCertificateStatusOutcome, QuinnCertificateStatusReason, QuinnNetworkFailureKind,
        QuinnRetryOutcome, QuinnRetryRequest,
    },
};

fn fp(ch: char) -> String {
    ch.to_string().repeat(64)
}

fn identity(fingerprint: String, surface: SurfaceScope) -> CertificateIdentity {
    CertificateIdentity::new(fingerprint, "server-a", surface).unwrap()
}

#[test]
fn quinn_admission_rejects_changed_fingerprint_without_rotation() {
    let mut planner = QuinnAdmissionPlanner::conservative().unwrap();
    let previous = ConnectionPoolKey::new(fp('a'), SurfacePlane::Application).unwrap();
    let presented = identity(fp('b'), SurfaceScope::Application);
    let mut request = QuinnAdmissionRequest::initial(
        &presented,
        SurfacePlane::Application,
        0,
        ZeroRttReplayClass::ReadOnlyManifest,
    );
    request.previous_pool_key = Some(&previous);

    let error = planner.admit_request(request).unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Security);
    assert!(error.message().contains("declared rotation"));
}

#[test]
fn quinn_admission_exposes_zero_rtt_unknown_idempotency_rejection() {
    let mut planner = QuinnAdmissionPlanner::conservative().unwrap();
    let presented = identity(fp('a'), SurfaceScope::Application);

    let decision = planner
        .admit_request(QuinnAdmissionRequest::initial(
            &presented,
            SurfacePlane::Application,
            0,
            ZeroRttReplayClass::UnknownIdempotency,
        ))
        .unwrap();

    assert_eq!(
        decision.certificate_status.outcome,
        QuinnCertificateStatusOutcome::Allowed
    );
    assert_eq!(
        decision.zero_rtt.rejection_reason(),
        Some(ZeroRttAdmissionRejectionReason::UnknownIdempotency)
    );
    assert!(
        decision.pool_admission.is_some(),
        "regular 1-RTT pool admission remains available after 0-RTT rejection"
    );
}

#[test]
fn quinn_admission_rejects_disabled_certificate_before_pool_admission() {
    let mut planner = QuinnAdmissionPlanner::conservative().unwrap();
    let presented = identity(fp('d'), SurfaceScope::Application);
    let request = QuinnAdmissionRequest::initial(
        &presented,
        SurfacePlane::Application,
        0,
        ZeroRttReplayClass::ReadOnlyManifest,
    )
    .with_certificate_status(CertificateIdentityStatus::Disabled);

    let error = planner.admit_request(request).unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Security);
    assert!(
        error.message().contains("certificate_disabled"),
        "disabled certificate denial must carry stable reason evidence"
    );
    assert!(
        error.message().contains("status=disabled"),
        "disabled certificate denial must carry status evidence"
    );
    assert!(
        planner.pool().is_empty(),
        "disabled certificate must not create or reuse a Quinn pool entry"
    );
}

#[test]
fn quinn_certificate_status_evidence_is_exhaustive_for_core_statuses() {
    let presented = identity(fp('e'), SurfaceScope::Application);

    let active =
        QuinnCertificateStatusEvidence::evaluate(&presented, CertificateIdentityStatus::Active)
            .unwrap();
    let disabled =
        QuinnCertificateStatusEvidence::evaluate(&presented, CertificateIdentityStatus::Disabled)
            .unwrap();
    let revoked =
        QuinnCertificateStatusEvidence::evaluate(&presented, CertificateIdentityStatus::Revoked)
            .unwrap();

    assert!(active.is_allowed());
    assert_eq!(active.reason, QuinnCertificateStatusReason::Active);
    assert!(disabled.is_denied());
    assert_eq!(
        disabled.reason,
        QuinnCertificateStatusReason::CertificateDisabled
    );
    assert!(revoked.is_denied());
    assert_eq!(
        revoked.reason,
        QuinnCertificateStatusReason::CertificateRevoked
    );
}

#[test]
fn quinn_admission_exposes_zero_rtt_hadr_demotion_rejection() {
    let mut planner = QuinnAdmissionPlanner::conservative().unwrap();
    let presented = identity(fp('a'), SurfaceScope::Cluster);

    let decision = planner
        .admit_request(QuinnAdmissionRequest::initial(
            &presented,
            SurfacePlane::HighAvailability,
            0,
            ZeroRttReplayClass::HadrDemotion,
        ))
        .unwrap();

    assert_eq!(
        decision.zero_rtt.rejection_reason(),
        Some(ZeroRttAdmissionRejectionReason::HadrDemotion)
    );
    assert_eq!(decision.pool_key.plane(), SurfacePlane::HighAvailability);
}

#[test]
fn quinn_admission_reuses_same_server_identity_and_plane_key() {
    let mut planner = QuinnAdmissionPlanner::conservative().unwrap();
    let presented = identity(fp('a'), SurfaceScope::Application);

    let first = planner
        .admit_request(QuinnAdmissionRequest::initial(
            &presented,
            SurfacePlane::Application,
            0,
            ZeroRttReplayClass::ReadOnlyManifest,
        ))
        .unwrap();
    let second = planner
        .admit_request(QuinnAdmissionRequest::initial(
            &presented,
            SurfacePlane::Application,
            10,
            ZeroRttReplayClass::ReadOnlyManifest,
        ))
        .unwrap();

    let first_pool = first.pool_admission.unwrap();
    let second_pool = second.pool_admission.unwrap();
    assert_eq!(first.pool_key, second.pool_key);
    assert_eq!(first_pool.kind, PoolAdmissionKind::Opened);
    assert_eq!(second_pool.kind, PoolAdmissionKind::Reused);
    assert_eq!(first_pool.connection_id, second_pool.connection_id);
}

#[test]
fn quinn_network_reset_maps_to_idempotent_retry_and_replacement_pool_entry() {
    let mut planner = QuinnAdmissionPlanner::conservative().unwrap();
    let presented = identity(fp('a'), SurfaceScope::Application);
    let opened = planner
        .admit_request(QuinnAdmissionRequest::initial(
            &presented,
            SurfacePlane::Application,
            0,
            ZeroRttReplayClass::ReadOnlyManifest,
        ))
        .unwrap()
        .pool_admission
        .unwrap();
    let mut request = QuinnAdmissionRequest::initial(
        &presented,
        SurfacePlane::Application,
        10,
        ZeroRttReplayClass::ReadOnlyManifest,
    );
    request.retry_after_failure = Some(QuinnRetryRequest {
        previous_connection_id: Some(opened.connection_id),
        failure: QuinnNetworkFailureKind::ConnectionReset,
        reconnect_state: ReconnectState::Disconnected,
        idempotency: RetryIdempotency::Idempotent,
        failed_attempt: 1,
    });

    let decision = planner.admit_request(request).unwrap();

    assert_eq!(
        decision.retry,
        QuinnRetryOutcome::RetryPolicy(RetryAdmissionDecision::Admit {
            next_attempt: 2,
            delay_ms: 200,
        })
    );
    let replacement = decision.pool_admission.unwrap();
    assert_eq!(replacement.kind, PoolAdmissionKind::Opened);
    assert_ne!(replacement.connection_id, opened.connection_id);
}

#[test]
fn quinn_network_reset_keeps_non_idempotent_request_out_of_retry_pool() {
    let mut planner = QuinnAdmissionPlanner::conservative().unwrap();
    let presented = identity(fp('a'), SurfaceScope::Application);
    let mut request = QuinnAdmissionRequest::initial(
        &presented,
        SurfacePlane::Application,
        0,
        ZeroRttReplayClass::MutatingProcedure,
    );
    request.retry_after_failure = Some(QuinnRetryRequest {
        previous_connection_id: None,
        failure: QuinnNetworkFailureKind::ConnectionReset,
        reconnect_state: ReconnectState::Disconnected,
        idempotency: RetryIdempotency::NonIdempotent,
        failed_attempt: 1,
    });

    let decision = planner.admit_request(request).unwrap();

    assert_eq!(
        decision.retry,
        QuinnRetryOutcome::RetryPolicy(RetryAdmissionDecision::Reject(
            RetryRejectionReason::NonIdempotentRequest
        ))
    );
    assert!(decision.pool_admission.is_none());
}

#[test]
fn quinn_protocol_failure_is_not_retryable_at_transport_boundary() {
    let mut planner = QuinnAdmissionPlanner::conservative().unwrap();
    let presented = identity(fp('a'), SurfaceScope::Application);
    let mut request = QuinnAdmissionRequest::initial(
        &presented,
        SurfacePlane::Application,
        0,
        ZeroRttReplayClass::ReadOnlyManifest,
    );
    request.retry_after_failure = Some(QuinnRetryRequest {
        previous_connection_id: None,
        failure: QuinnNetworkFailureKind::ProtocolViolation,
        reconnect_state: ReconnectState::Disconnected,
        idempotency: RetryIdempotency::Idempotent,
        failed_attempt: 1,
    });

    let decision = planner.admit_request(request).unwrap();

    assert_eq!(
        decision.retry,
        QuinnRetryOutcome::TransportNotRetryable {
            failure: QuinnNetworkFailureKind::ProtocolViolation,
        }
    );
    assert!(decision.pool_admission.is_none());
}
