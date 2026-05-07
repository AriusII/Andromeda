use std::collections::HashSet;

use andromeda_core::{
    CertificateFingerprint, CertificateIdentity, Permission, PermissionSet, Principal,
    PrincipalBinding, PrincipalId, PrincipalRole, ProcedureId, SessionToken, SurfaceScope,
};

use crate::principal_fixtures::{
    seeded_fingerprint, test_fingerprint, test_principal, test_superadmin_principal,
};

#[test]
fn test_principal_id_non_zero_required() {
    let zero_id = PrincipalId::new(0);
    assert!(zero_id.is_zero(), "zero ID must be detected");

    let nonzero_id = PrincipalId::new(1);
    assert!(!nonzero_id.is_zero(), "non-zero ID must be valid");
}

#[test]
fn test_principal_id_equality_and_hashing() {
    let id1 = PrincipalId::new(42);
    let id2 = PrincipalId::new(42);
    let id3 = PrincipalId::new(43);

    assert_eq!(id1, id2, "same IDs must be equal");
    assert_ne!(id1, id3, "different IDs must not be equal");

    let mut ids = HashSet::new();
    ids.insert(id1);
    ids.insert(id2);
    assert_eq!(ids.len(), 1, "duplicate IDs must deduplicate in set");
}

#[test]
fn test_session_token_immutability() {
    let token = SessionToken::new("immutable-token-42");
    let token_str_1 = token.as_str();
    let token_str_2 = token.as_str();

    assert_eq!(token_str_1, token_str_2, "token string must be stable");
    assert_eq!(
        token_str_1, "immutable-token-42",
        "token must preserve content"
    );
}

#[test]
fn test_session_token_empty_detection() {
    let empty = SessionToken::new("");
    let nonempty = SessionToken::new("token");

    assert!(empty.is_empty(), "empty token must be detected");
    assert!(!nonempty.is_empty(), "non-empty token must be valid");
}

#[test]
fn test_certificate_fingerprint_sha256_validation() {
    let valid = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
    let fp = CertificateFingerprint::new(valid).expect("valid SHA-256");
    assert!(fp.is_valid_sha256(), "must validate SHA-256 format");

    let short = "a1b2c3d4";
    let fp_short = CertificateFingerprint::new(short).expect("short fingerprint");
    assert!(
        !fp_short.is_valid_sha256(),
        "short fingerprint must not validate"
    );

    let non_hex = "g1g2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
    let fp_non_hex = CertificateFingerprint::new(non_hex).expect("non-hex fingerprint");
    assert!(!fp_non_hex.is_valid_sha256(), "non-hex must not validate");
}

#[test]
fn test_certificate_fingerprint_empty_rejected() {
    let empty = CertificateFingerprint::new("");
    assert!(empty.is_none(), "empty fingerprint must be rejected");

    let whitespace = CertificateFingerprint::new("   ");
    assert!(
        whitespace.is_none(),
        "whitespace-only fingerprint must be rejected"
    );
}

#[test]
fn test_principal_creation_validates_non_zero_id() {
    let fingerprint = CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
    let token = SessionToken::new("token");

    let zero_result = Principal::new(
        PrincipalId::new(0),
        PrincipalRole::User,
        token.clone(),
        fingerprint.clone(),
    );
    assert!(zero_result.is_none(), "zero principal ID must be rejected");

    let nonzero_result =
        Principal::new(PrincipalId::new(1), PrincipalRole::User, token, fingerprint);
    assert!(
        nonzero_result.is_some(),
        "non-zero principal ID must be accepted"
    );
}

#[test]
fn test_principal_creation_validates_non_empty_token() {
    let fingerprint = CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
    let id = PrincipalId::new(42);

    let empty_token_result = Principal::new(
        id,
        PrincipalRole::User,
        SessionToken::new(""),
        fingerprint.clone(),
    );
    assert!(
        empty_token_result.is_none(),
        "empty session token must be rejected"
    );

    let nonempty_token_result = Principal::new(
        id,
        PrincipalRole::User,
        SessionToken::new("token"),
        fingerprint,
    );
    assert!(
        nonempty_token_result.is_some(),
        "non-empty session token must be accepted"
    );
}

#[test]
fn test_principal_creation_uses_checked_certificate_fingerprint() {
    assert!(CertificateFingerprint::new("").is_none());
    assert!(CertificateFingerprint::new("   ").is_none());

    let fingerprint = CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
    let token = SessionToken::new("token");
    let id = PrincipalId::new(42);

    assert!(Principal::new(id, PrincipalRole::User, token, fingerprint).is_some());
}

#[test]
fn test_principal_deterministic_creation() {
    let fp = CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
    let token = SessionToken::new("same-token");
    let id = PrincipalId::new(123);

    let p1 = Principal::new(id, PrincipalRole::Operator, token.clone(), fp.clone())
        .expect("first principal");
    let p2 = Principal::new(id, PrincipalRole::Operator, token, fp).expect("second principal");

    assert_eq!(p1.id, p2.id, "principal IDs must match");
    assert_eq!(p1.role, p2.role, "principal roles must match");
    assert_eq!(
        p1.session_token, p2.session_token,
        "session tokens must match"
    );
    assert_eq!(
        p1.cert_fingerprint, p2.cert_fingerprint,
        "fingerprints must match"
    );
    assert!(
        p1.created_at <= std::time::SystemTime::now(),
        "created_at must not be in future"
    );
    assert!(
        p2.created_at <= std::time::SystemTime::now(),
        "created_at must not be in future"
    );
}

#[test]
fn test_principal_permissions_by_role() {
    let superadmin = test_superadmin_principal();
    let user = test_principal();

    let exec_all = Permission::ExecuteProcedure(ProcedureId::new(u64::MAX));
    assert!(
        superadmin.has_permission(&exec_all),
        "superadmin must execute all procedures"
    );
    assert!(
        user.has_permission(&exec_all),
        "user must execute procedures"
    );
    assert!(
        !user.has_permission(&Permission::AdminShutdown),
        "user must not have admin shutdown"
    );
    assert!(
        superadmin.has_permission(&Permission::AdminShutdown),
        "superadmin must have admin shutdown"
    );
}

#[test]
fn test_principal_binding_rejects_certificate_user_fingerprint_mismatch() {
    let certificate = CertificateIdentity::new(
        seeded_fingerprint('a'),
        "CN=svc-mismatch",
        SurfaceScope::Application,
    )
    .expect("valid certificate identity");
    let mismatched_fingerprint =
        CertificateFingerprint::new(seeded_fingerprint('b')).expect("valid fingerprint");
    let principal = Principal::new(
        PrincipalId::new(814),
        PrincipalRole::User,
        SessionToken::from_certificate_fingerprint(&mismatched_fingerprint),
        mismatched_fingerprint,
    )
    .expect("active principal");

    let binding = PrincipalBinding::new(certificate, principal);

    assert!(
        binding.is_err(),
        "certificate identity and user principal fingerprint evidence must match"
    );
}

#[test]
fn test_permission_set_matching() {
    let perms = PermissionSet::new()
        .with_permission(Permission::ExecuteProcedure(ProcedureId::new(1)))
        .with_permission(Permission::ReadContractMetadata);

    assert!(
        perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(1))),
        "exact permission match must succeed"
    );
    assert!(
        !perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(u64::MAX))),
        "ExecuteProcedure(1) must not match ExecuteProcedure(u64::MAX) requirement"
    );
    assert!(
        !perms.has_permission(&Permission::AdminRecovery),
        "missing permission must not match"
    );
}

#[test]
fn test_permission_set_default_deny() {
    let empty_perms = PermissionSet::new();

    assert!(
        empty_perms.is_empty(),
        "empty permission set must be detected"
    );
    assert!(
        !empty_perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(1))),
        "empty permission set must deny all"
    );
}

#[test]
fn test_session_token_immutable_during_principal_lifetime() {
    let p = test_principal();
    let token_1 = p.session_token.clone();
    let token_2 = p.session_token.clone();

    assert_eq!(token_1, token_2, "session token must not change");
    assert_eq!(
        token_1.as_str(),
        token_2.as_str(),
        "token string must be stable"
    );
}

#[test]
fn test_role_immutable_during_principal_lifetime() {
    let p = test_principal();
    let role_1 = p.role;
    let role_2 = p.role;

    assert_eq!(role_1, role_2, "role must not change");
}

#[test]
fn test_masked_display_hides_fingerprint() {
    let p = test_principal();
    let display = p.masked_display();

    assert!(
        display.contains(&format!("{}", p.id)),
        "display must include principal ID"
    );
    assert!(display.contains("User"), "display must include role");
    assert!(
        !display.contains(test_fingerprint().as_str()),
        "display must not expose full fingerprint"
    );

    let partial = &p.cert_fingerprint.as_str()[..6.min(p.cert_fingerprint.len())];
    assert!(
        display.contains(partial),
        "display must show partial fingerprint"
    );
}

#[test]
fn test_principal_role_string_conversion() {
    assert_eq!(PrincipalRole::SuperAdmin.as_str(), "superadmin");
    assert_eq!(PrincipalRole::Admin.as_str(), "admin");
    assert_eq!(PrincipalRole::Operator.as_str(), "operator");
    assert_eq!(PrincipalRole::User.as_str(), "user");
    assert_eq!(PrincipalRole::Guest.as_str(), "guest");
}

#[test]
fn test_principal_role_from_string() {
    assert_eq!(
        "superadmin".parse::<PrincipalRole>().ok(),
        Some(PrincipalRole::SuperAdmin)
    );
    assert_eq!(
        "user".parse::<PrincipalRole>().ok(),
        Some(PrincipalRole::User)
    );
    assert_eq!("invalid".parse::<PrincipalRole>().ok(), None);
}

#[test]
fn test_principal_role_permissions() {
    let super_perms = PrincipalRole::SuperAdmin.permissions();
    assert!(!super_perms.is_empty(), "superadmin must have permissions");
    assert!(
        super_perms.has_permission(&Permission::AdminShutdown),
        "superadmin must have shutdown"
    );

    let guest_perms = PrincipalRole::Guest.permissions();
    assert!(
        guest_perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(0))),
        "guest must have public procedure access"
    );
}

#[test]
fn test_full_principal_creation_and_permission_check() {
    let fingerprint = CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
    let token = SessionToken::new("integration-test-001");
    let id = PrincipalId::new(999);

    let principal = Principal::new(
        id,
        PrincipalRole::Operator,
        token.clone(),
        fingerprint.clone(),
    )
    .expect("principal created");

    assert!(!principal.id.is_zero());
    assert!(!principal.session_token.is_empty());
    assert!(!principal.cert_fingerprint.is_empty());
    assert_eq!(principal.role, PrincipalRole::Operator);
    assert!(
        principal.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(u64::MAX))),
        "operator must execute procedures"
    );
    assert!(
        principal.has_permission(&Permission::AuditRead),
        "operator must read audit"
    );
    assert!(
        !principal.has_permission(&Permission::AdminShutdown),
        "operator must not shutdown"
    );
}

#[test]
fn test_principal_clone_independence() {
    let p1 = test_principal();
    let p2 = p1.clone();

    assert_eq!(p1, p2);
    assert_eq!(p1.id, p2.id);
    assert_eq!(p1.session_token, p2.session_token);
    assert_eq!(p1.cert_fingerprint, p2.cert_fingerprint);
}
