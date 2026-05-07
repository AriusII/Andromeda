use andromeda_core::{
    CertificateFingerprint, CertificateIdentity, PermissionSet, Principal, PrincipalBinding,
    PrincipalId, PrincipalRole, PrincipalStatus, SessionToken, SurfaceScope,
};

pub(crate) fn test_fingerprint() -> String {
    "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string()
}

pub(crate) fn seeded_fingerprint(seed: char) -> String {
    debug_assert!(seed.is_ascii_hexdigit());
    format!("{seed}1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2")
}

pub(crate) fn test_principal() -> Principal {
    let fingerprint = CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
    let token = SessionToken::new("session-test-001");
    let id = PrincipalId::new(42);

    Principal::new(id, PrincipalRole::User, token, fingerprint).expect("valid principal")
}

pub(crate) fn test_superadmin_principal() -> Principal {
    let fingerprint = CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
    let token = SessionToken::new("session-admin-001");
    let id = PrincipalId::new(100);

    Principal::new(id, PrincipalRole::SuperAdmin, token, fingerprint).expect("valid principal")
}

pub(crate) fn certificate_identity(
    fingerprint: String,
    subject: &str,
    surface_scope: SurfaceScope,
) -> CertificateIdentity {
    CertificateIdentity::new(fingerprint, subject, surface_scope)
        .expect("valid certificate identity")
}

pub(crate) fn principal_for_certificate(
    certificate: &CertificateIdentity,
    principal_id: u64,
    role: PrincipalRole,
    status: PrincipalStatus,
) -> Principal {
    Principal::new_with_status(
        PrincipalId::new(principal_id),
        role,
        status,
        SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
        certificate.fingerprint().clone(),
    )
    .expect("principal has valid identity evidence")
}

pub(crate) fn principal_binding(
    fingerprint: String,
    subject: &str,
    surface_scope: SurfaceScope,
    principal_id: u64,
    role: PrincipalRole,
    status: PrincipalStatus,
) -> PrincipalBinding {
    let certificate = certificate_identity(fingerprint, subject, surface_scope);
    let principal = principal_for_certificate(&certificate, principal_id, role, status);
    PrincipalBinding::new(certificate, principal).expect("valid principal binding")
}

pub(crate) fn principal_binding_with_direct_permissions(
    fingerprint: String,
    subject: &str,
    surface_scope: SurfaceScope,
    principal_id: u64,
    role: PrincipalRole,
    status: PrincipalStatus,
    direct_permissions: PermissionSet,
) -> PrincipalBinding {
    let certificate = certificate_identity(fingerprint, subject, surface_scope);
    let principal = principal_for_certificate(&certificate, principal_id, role, status);
    PrincipalBinding::new_with_direct_permissions(certificate, principal, direct_permissions)
        .expect("valid principal binding")
}
