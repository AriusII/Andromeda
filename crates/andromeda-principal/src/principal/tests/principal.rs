use super::*;
use andromeda_types::ProcedureId;
use helpers::{fingerprint, seeded_sha256, user_principal};

#[test]
fn test_principal_creation_success() {
    let id = PrincipalId::new(1);
    let token = SessionToken::new("test-token");
    let fp = fingerprint("fingerprint-123");

    let principal = Principal::new(id, PrincipalRole::User, token, fp);
    assert!(principal.is_some());

    let p = principal.unwrap();
    assert_eq!(p.id, id);
    assert_eq!(p.role, PrincipalRole::User);
    assert_eq!(p.cert_fingerprint.as_str(), "fingerprint-123");
}

#[test]
fn test_principal_creation_zero_id_rejected() {
    let id = PrincipalId::new(0);
    let token = SessionToken::new("test-token");
    let fp = fingerprint("fingerprint");

    let principal = Principal::new(id, PrincipalRole::User, token, fp);
    assert!(principal.is_none());
}

#[test]
fn test_principal_creation_empty_token_rejected() {
    let id = PrincipalId::new(1);
    let token = SessionToken::new("");
    let fp = fingerprint("fingerprint");

    let principal = Principal::new(id, PrincipalRole::User, token, fp);
    assert!(principal.is_none());
}

#[test]
fn test_principal_creation_empty_fingerprint_rejected() {
    let id = PrincipalId::new(1);
    let token = SessionToken::new("test-token");
    let fp = CertificateFingerprint::new_unchecked("");

    let principal = Principal::new(id, PrincipalRole::User, token, fp);
    assert!(principal.is_none());
}

#[test]
fn test_principal_try_new_returns_typed_security_errors() {
    let token = SessionToken::new("test-token");
    let fp = fingerprint("fingerprint");

    let error = Principal::try_new(PrincipalId::new(0), PrincipalRole::User, token, fp)
        .expect_err("zero principal id must be rejected");

    assert_eq!(error.kind(), andromeda_error::AndromedaErrorKind::Security);
}

#[test]
fn test_principal_has_permission() {
    let principal = Principal::new(
        PrincipalId::new(1),
        PrincipalRole::Admin,
        SessionToken::new("test-token"),
        fingerprint("fingerprint"),
    )
    .expect("Principal creation should succeed");

    assert!(principal.has_permission(&Permission::AdminCatalogPublish));
    assert!(!principal.has_permission(&Permission::AdminShutdown));
}

#[test]
fn test_principal_permissions_immutable() {
    let principal = user_principal();

    let perms1 = principal.permissions();
    let perms2 = principal.permissions();
    assert_eq!(perms1, perms2);
}

#[test]
fn test_principal_masked_display() {
    let principal = Principal::new(
        PrincipalId::new(1),
        PrincipalRole::Admin,
        SessionToken::new("test-token"),
        fingerprint("abcdef1234567890"),
    )
    .expect("Principal creation should succeed");

    let masked = principal.masked_display();
    assert!(masked.contains("PrincipalId(1)"));
    assert!(masked.contains("admin"));
    assert!(masked.contains("abcdef***"));
}

#[test]
fn test_principal_masked_display_utf8_safe() {
    let principal = Principal::new(
        PrincipalId::new(1),
        PrincipalRole::Admin,
        SessionToken::new("test-token"),
        fingerprint("abcdeé123456"),
    )
    .expect("Principal creation should succeed");

    let masked = principal.masked_display();
    assert!(masked.contains("abcdeé***"));
    assert!(!masked.contains("abcdeé123456"));
}

#[test]
fn test_principal_with_timestamp() {
    use std::time::UNIX_EPOCH;

    let timestamp = UNIX_EPOCH;
    let principal = Principal::new_with_timestamp(
        PrincipalId::new(1),
        PrincipalRole::User,
        SessionToken::new("test-token"),
        fingerprint("fingerprint"),
        timestamp,
    )
    .expect("Principal creation should succeed");

    assert_eq!(principal.created_at, timestamp);
}

#[test]
fn test_principal_superadmin_bypass_permissions() {
    let principal = Principal::new(
        PrincipalId::new(999),
        PrincipalRole::SuperAdmin,
        SessionToken::new("superadmin-token"),
        fingerprint("sa-fingerprint"),
    )
    .expect("Principal creation should succeed");

    assert!(principal.has_permission(&Permission::AdminShutdown));
    assert!(principal.has_permission(&Permission::AdminRecovery));
    assert!(principal.has_permission(&Permission::AdminCertificateRotate));
    assert!(principal.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
}

#[test]
fn test_disabled_principal_denies_all_role_permissions() {
    let principal = Principal::new_with_status(
        PrincipalId::new(1001),
        PrincipalRole::SuperAdmin,
        PrincipalStatus::Disabled,
        SessionToken::new("disabled-token"),
        fingerprint(seeded_sha256('1')),
    )
    .expect("disabled principal still carries audit identity");

    assert!(!principal.is_active());
    assert!(principal.permissions().is_empty());
    assert!(!principal.has_permission(&Permission::AdminShutdown));
    assert!(!principal.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
}

#[test]
fn test_multiple_principals_independent() {
    let user1 = Principal::new(
        PrincipalId::new(1),
        PrincipalRole::User,
        SessionToken::new("token1"),
        fingerprint("fp1"),
    )
    .unwrap();

    let user2 = Principal::new(
        PrincipalId::new(2),
        PrincipalRole::Guest,
        SessionToken::new("token2"),
        fingerprint("fp2"),
    )
    .unwrap();

    assert_ne!(user1.id, user2.id);
    assert_ne!(user1.role, user2.role);
    assert_ne!(user1.session_token, user2.session_token);
}
