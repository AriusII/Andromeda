//! Runtime-free reconnect certificate continuity contract.

use andromeda_core::AndromedaErrorKind;
use andromeda_core::{CertificateIdentity, SurfaceScope};
use andromeda_quic::{
    CertificateContinuityDecision, CertificateContinuityPolicy, CertificateRotationDeclaration,
    ConnectionPool, ConnectionPoolKey, ConnectionPoolPolicy, PoolAdmissionKind, SurfacePlane,
};

fn fp(ch: char) -> String {
    ch.to_string().repeat(64)
}

fn identity(fingerprint: String, surface: SurfaceScope) -> CertificateIdentity {
    CertificateIdentity::new(fingerprint, "server-a", surface).unwrap()
}

#[test]
fn reconnect_accepts_same_server_fingerprint() {
    let previous = ConnectionPoolKey::new(fp('a'), SurfacePlane::Application).unwrap();
    let presented = identity(fp('a'), SurfaceScope::Application);

    let decision = CertificateContinuityPolicy::strict()
        .validate_reconnect(&previous, &presented, SurfacePlane::Application, None)
        .unwrap();

    assert_eq!(
        decision,
        CertificateContinuityDecision::AcceptSameFingerprint
    );
}

#[test]
fn reconnect_rejects_changed_fingerprint_without_rotation() {
    let previous = ConnectionPoolKey::new(fp('a'), SurfacePlane::Application).unwrap();
    let presented = identity(fp('b'), SurfaceScope::Application);

    let err = CertificateContinuityPolicy::strict()
        .validate_reconnect(&previous, &presented, SurfacePlane::Application, None)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(
        err.message().contains("declared rotation"),
        "error should require declared rotation"
    );
}

#[test]
fn reconnect_allows_declared_rotation() {
    let previous = ConnectionPoolKey::new(fp('a'), SurfacePlane::Application).unwrap();
    let presented = identity(fp('b'), SurfaceScope::Application);
    let rotation =
        CertificateRotationDeclaration::new(fp('a'), fp('b'), SurfacePlane::Application).unwrap();

    let decision = CertificateContinuityPolicy::allow_declared_rotation()
        .validate_reconnect(
            &previous,
            &presented,
            SurfacePlane::Application,
            Some(&rotation),
        )
        .unwrap();

    assert_eq!(
        decision,
        CertificateContinuityDecision::AcceptDeclaredRotation
    );
}

#[test]
fn pool_key_separates_surface_planes() {
    let mut pool = ConnectionPool::new(ConnectionPoolPolicy {
        max_connections_per_key: 1,
        idle_timeout_ms: 1_000,
        evict_unhealthy: true,
    })
    .unwrap();

    let app_key = ConnectionPoolKey::new(fp('a'), SurfacePlane::Application).unwrap();
    let admin_key = ConnectionPoolKey::new(fp('a'), SurfacePlane::Administration).unwrap();
    let hadr_key = ConnectionPoolKey::new(fp('a'), SurfacePlane::HighAvailability).unwrap();

    let app = pool.admit_or_reuse(app_key.clone(), 0).unwrap();
    let app_reused = pool.admit_or_reuse(app_key, 10).unwrap();
    let admin = pool.admit_or_reuse(admin_key, 10).unwrap();
    let hadr = pool.admit_or_reuse(hadr_key, 10).unwrap();

    assert_eq!(app.kind, PoolAdmissionKind::Opened);
    assert_eq!(app_reused.kind, PoolAdmissionKind::Reused);
    assert_eq!(admin.kind, PoolAdmissionKind::Opened);
    assert_eq!(hadr.kind, PoolAdmissionKind::Opened);
    assert_ne!(admin.connection_id, app.connection_id);
    assert_ne!(hadr.connection_id, app.connection_id);
    assert_ne!(hadr.connection_id, admin.connection_id);
}

#[test]
fn reconnect_rejects_same_fingerprint_on_different_surface_plane() {
    let previous = ConnectionPoolKey::new(fp('a'), SurfacePlane::Application).unwrap();
    let presented = identity(fp('a'), SurfaceScope::Cluster);

    let err = CertificateContinuityPolicy::strict()
        .validate_reconnect(&previous, &presented, SurfacePlane::HighAvailability, None)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(
        err.message().contains("plane"),
        "error should preserve pool-key plane separation"
    );
}
