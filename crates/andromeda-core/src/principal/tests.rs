use super::*;
use crate::ProcedureId;

#[test]
fn test_principal_id_creation() {
    let id = PrincipalId::new(42);
    assert_eq!(id.get(), 42);
    assert!(!id.is_zero());
}

#[test]
fn test_principal_id_zero() {
    let id = PrincipalId::new(0);
    assert!(id.is_zero());
}

#[test]
fn test_principal_id_display() {
    let id = PrincipalId::new(42);
    assert_eq!(id.to_string(), "PrincipalId(42)");
}

#[test]
fn test_principal_id_from_u64() {
    let id = PrincipalId::from(123u64);
    assert_eq!(id.get(), 123);
}

#[test]
fn test_session_token_creation() {
    let token = SessionToken::new("test-token-123");
    assert_eq!(token.as_str(), "test-token-123");
    assert!(!token.is_empty());
}

#[test]
fn test_session_token_empty() {
    let token = SessionToken::new("");
    assert!(token.is_empty());
}

#[test]
fn test_session_token_display() {
    let token = SessionToken::new("my-session");
    assert_eq!(token.to_string(), "my-session");
}

#[test]
fn test_certificate_derived_session_token_is_truncated_and_prefixed() {
    let fp = CertificateFingerprint::new(
        "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
    )
    .unwrap();

    let token = SessionToken::from_certificate_fingerprint(&fp);

    assert_eq!(token.as_str(), "mtls:a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4");
    assert!(
        !token.as_str().contains(&fp.as_str()[32..]),
        "derived token must not embed the full fingerprint"
    );
}

#[test]
fn test_certificate_derived_session_token_is_non_secret_evidence() {
    let fp = CertificateFingerprint::new(
        "ffffffffe5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
    )
    .unwrap();

    let token = SessionToken::from_certificate_fingerprint(&fp);

    assert_eq!(
        token.to_string(),
        token.as_str(),
        "session token display is intentionally unmasked audit evidence, not a bearer secret"
    );
}

#[test]
fn test_certificate_derived_session_token_from_zero_fingerprint_is_valid_evidence() {
    let fp = CertificateFingerprint::new(
        "0000000000000000000000000000000000000000000000000000000000000000",
    )
    .unwrap();

    let token = SessionToken::from_certificate_fingerprint(&fp);

    assert_eq!(token.as_str(), "mtls:00000000000000000000000000000000");
    assert!(!token.is_empty());
}

#[test]
fn test_certificate_fingerprint_creation() {
    let fp = CertificateFingerprint::new("a1b2c3d4").unwrap();
    assert_eq!(fp.as_str(), "a1b2c3d4");
}

#[test]
fn test_certificate_fingerprint_empty_rejected() {
    let fp = CertificateFingerprint::new("");
    assert!(fp.is_none());
}

#[test]
fn test_certificate_fingerprint_whitespace_only_rejected() {
    let fp = CertificateFingerprint::new("   ");
    assert!(fp.is_none());
}

#[test]
fn test_certificate_fingerprint_creation_trims_edges() {
    let fp = CertificateFingerprint::new("  a1b2c3d4  ").unwrap();
    assert_eq!(fp.as_str(), "a1b2c3d4");
}

#[test]
fn test_certificate_fingerprint_valid_sha256() {
    let valid_sha256 = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
    let fp = CertificateFingerprint::new(valid_sha256).unwrap();
    assert!(fp.is_valid_sha256());
}

#[test]
fn test_certificate_fingerprint_invalid_sha256_too_short() {
    let short = "a1b2c3d4e5f6";
    let fp = CertificateFingerprint::new(short).unwrap();
    assert!(!fp.is_valid_sha256());
}

#[test]
fn test_certificate_fingerprint_invalid_sha256_non_hex() {
    let non_hex = "g1g2g3g4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6g1g2";
    let fp = CertificateFingerprint::new(non_hex).unwrap();
    assert!(!fp.is_valid_sha256());
}

#[test]
fn test_certificate_fingerprint_len() {
    let fp = CertificateFingerprint::new("test").unwrap();
    assert_eq!(fp.len(), 4);
}

#[test]
fn test_certificate_fingerprint_display() {
    let fp = CertificateFingerprint::new("abc123").unwrap();
    assert_eq!(fp.to_string(), "abc123");
}

#[test]
fn test_certificate_fingerprint_new_accepts_owned_string() {
    let fp = CertificateFingerprint::new("test".to_string()).unwrap();
    assert_eq!(fp.as_str(), "test");
}

#[test]
fn test_certificate_fingerprint_new_accepts_borrowed_str() {
    let fp = CertificateFingerprint::new("test").unwrap();
    assert_eq!(fp.as_str(), "test");
}

#[test]
fn test_certificate_fingerprint_unchecked() {
    let fp = CertificateFingerprint::new_unchecked("");
    assert!(fp.is_empty());
}

#[test]
fn test_principal_id_from_sha256_fingerprint_uses_core_rule() {
    let fp = CertificateFingerprint::new(
        "00000000e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
    )
    .unwrap();

    let id = PrincipalId::from_certificate_fingerprint(&fp).unwrap();

    assert_eq!(id.get(), 1, "zero-derived ids must be remapped to one");
}

#[test]
fn test_principal_id_from_freeform_fingerprint_preserves_legacy_dev_derivation() {
    let fp = CertificateFingerprint::new("test_user_fingerprint").unwrap();

    let id = PrincipalId::from_certificate_fingerprint(&fp).unwrap();
    let expected = fp.as_str().as_bytes().iter().fold(0u64, |acc, &byte| {
        acc.wrapping_mul(31).wrapping_add(byte as u64)
    });

    assert_eq!(id.get(), expected);
}

#[test]
fn test_principal_role_str_conversion() {
    assert_eq!(PrincipalRole::SuperAdmin.as_str(), "superadmin");
    assert_eq!(PrincipalRole::Admin.as_str(), "admin");
    assert_eq!(PrincipalRole::Operator.as_str(), "operator");
    assert_eq!(PrincipalRole::User.as_str(), "user");
    assert_eq!(PrincipalRole::Guest.as_str(), "guest");
}

#[test]
fn test_principal_role_from_str() {
    assert_eq!(
        "superadmin".parse::<PrincipalRole>().ok(),
        Some(PrincipalRole::SuperAdmin)
    );
    assert_eq!(
        "admin".parse::<PrincipalRole>().ok(),
        Some(PrincipalRole::Admin)
    );
    assert_eq!(
        "user".parse::<PrincipalRole>().ok(),
        Some(PrincipalRole::User)
    );
    assert_eq!("invalid".parse::<PrincipalRole>().ok(), None);
}

#[test]
fn test_principal_role_display() {
    assert_eq!(PrincipalRole::SuperAdmin.to_string(), "superadmin");
    assert_eq!(PrincipalRole::Operator.to_string(), "operator");
}

#[test]
fn test_permission_execute_procedure() {
    let perm = Permission::ExecuteProcedure(ProcedureId::new(42));
    assert_eq!(perm.as_str(), "execute_procedure");
}

#[test]
fn test_permission_matches_exact() {
    let perm = Permission::ExecuteProcedure(ProcedureId::new(42));
    let required = Permission::ExecuteProcedure(ProcedureId::new(42));
    assert!(perm.matches(&required));
}

#[test]
fn test_permission_matches_wildcard() {
    let perm = Permission::ExecuteProcedure(ProcedureId::new(u64::MAX));
    let required = Permission::ExecuteProcedure(ProcedureId::new(42));
    assert!(perm.matches(&required));
}

#[test]
fn test_permission_does_not_match() {
    let perm = Permission::ExecuteProcedure(ProcedureId::new(10));
    let required = Permission::ExecuteProcedure(ProcedureId::new(42));
    assert!(!perm.matches(&required));
}

#[test]
fn test_permission_display() {
    let perm = Permission::ExecuteProcedure(ProcedureId::new(42));
    assert_eq!(perm.to_string(), "execute_procedure(42)");

    let perm2 = Permission::AdminCatalogPublish;
    assert_eq!(perm2.to_string(), "admin_catalog_publish");
}

#[test]
fn test_permission_set_has_permission() {
    let set = PermissionSet::new()
        .with_permission(Permission::AdminCatalogPublish)
        .with_permission(Permission::AuditRead);

    assert!(set.has_permission(&Permission::AdminCatalogPublish));
    assert!(set.has_permission(&Permission::AuditRead));
    assert!(!set.has_permission(&Permission::AdminShutdown));
}

#[test]
fn test_permission_set_empty() {
    let set = PermissionSet::new();
    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
}

#[test]
fn test_permission_set_from_vec() {
    let perms = vec![
        Permission::AdminCatalogPublish,
        Permission::AuditRead,
        Permission::AuditRead,
    ];
    let set = PermissionSet::from_vec(perms);
    assert_eq!(set.len(), 2);
}

#[test]
fn test_super_admin_has_all_permissions() {
    let perms = PrincipalRole::SuperAdmin.permissions();
    assert!(perms.has_permission(&Permission::AdminShutdown));
    assert!(perms.has_permission(&Permission::AdminRecovery));
    assert!(perms.has_permission(&Permission::AuditRead));
    assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
}

#[test]
fn test_guest_limited_permissions() {
    let perms = PrincipalRole::Guest.permissions();
    assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(0))));
    assert!(!perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
}

#[test]
fn test_operator_permissions() {
    let perms = PrincipalRole::Operator.permissions();
    assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
    assert!(perms.has_permission(&Permission::AuditRead));
    assert!(!perms.has_permission(&Permission::AdminShutdown));
}

#[test]
fn test_admin_permissions() {
    let perms = PrincipalRole::Admin.permissions();
    assert!(perms.has_permission(&Permission::AdminRoleManagement));
    assert!(!perms.has_permission(&Permission::AdminShutdown));
}

#[test]
fn test_user_permissions() {
    let perms = PrincipalRole::User.permissions();
    assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
    assert!(perms.has_permission(&Permission::ReadContractMetadata));
    assert!(!perms.has_permission(&Permission::AdminRoleManagement));
}

#[test]
fn test_principal_creation_success() {
    let id = PrincipalId::new(1);
    let token = SessionToken::new("test-token");
    let fp = CertificateFingerprint::new("fingerprint-123").unwrap();

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
    let fp = CertificateFingerprint::new("fingerprint").unwrap();

    let principal = Principal::new(id, PrincipalRole::User, token, fp);
    assert!(principal.is_none());
}

#[test]
fn test_principal_creation_empty_token_rejected() {
    let id = PrincipalId::new(1);
    let token = SessionToken::new("");
    let fp = CertificateFingerprint::new("fingerprint").unwrap();

    let principal = Principal::new(id, PrincipalRole::User, token, fp);
    assert!(principal.is_none());
}

#[test]
fn test_principal_creation_empty_fingerprint_rejected() {
    let id = PrincipalId::new(1);
    let token = SessionToken::new("test-token");
    let fp = CertificateFingerprint::new("").unwrap_or(CertificateFingerprint::new_unchecked(""));

    let principal = Principal::new(id, PrincipalRole::User, token, fp);
    assert!(principal.is_none());
}

#[test]
fn test_principal_try_new_returns_typed_security_errors() {
    let token = SessionToken::new("test-token");
    let fp = CertificateFingerprint::new("fingerprint").unwrap();

    let error = Principal::try_new(PrincipalId::new(0), PrincipalRole::User, token, fp)
        .expect_err("zero principal id must be rejected");

    assert_eq!(error.kind(), crate::AndromedaErrorKind::Security);
}

#[test]
fn test_principal_has_permission() {
    let id = PrincipalId::new(1);
    let token = SessionToken::new("test-token");
    let fp = CertificateFingerprint::new("fingerprint").unwrap();

    let principal = Principal::new(id, PrincipalRole::Admin, token, fp)
        .expect("Principal creation should succeed");

    assert!(principal.has_permission(&Permission::AdminCatalogPublish));
    assert!(!principal.has_permission(&Permission::AdminShutdown));
}

#[test]
fn test_principal_permissions_immutable() {
    let id = PrincipalId::new(1);
    let token = SessionToken::new("test-token");
    let fp = CertificateFingerprint::new("fingerprint").unwrap();

    let principal = Principal::new(id, PrincipalRole::User, token, fp)
        .expect("Principal creation should succeed");

    let perms1 = principal.permissions();
    let perms2 = principal.permissions();
    assert_eq!(perms1, perms2);
}

#[test]
fn test_principal_masked_display() {
    let id = PrincipalId::new(1);
    let token = SessionToken::new("test-token");
    let fp = CertificateFingerprint::new("abcdef1234567890").unwrap();

    let principal = Principal::new(id, PrincipalRole::Admin, token, fp)
        .expect("Principal creation should succeed");

    let masked = principal.masked_display();
    assert!(masked.contains("PrincipalId(1)"));
    assert!(masked.contains("admin"));
    assert!(masked.contains("abcdef***"));
}

#[test]
fn test_principal_masked_display_utf8_safe() {
    let id = PrincipalId::new(1);
    let token = SessionToken::new("test-token");
    let fp = CertificateFingerprint::new("abcdeé123456").unwrap();

    let principal = Principal::new(id, PrincipalRole::Admin, token, fp)
        .expect("Principal creation should succeed");

    let masked = principal.masked_display();
    assert!(masked.contains("abcdeé***"));
    assert!(!masked.contains("abcdeé123456"));
}

#[test]
fn test_principal_with_timestamp() {
    use std::time::UNIX_EPOCH;

    let id = PrincipalId::new(1);
    let token = SessionToken::new("test-token");
    let fp = CertificateFingerprint::new("fingerprint").unwrap();
    let timestamp = UNIX_EPOCH;

    let principal = Principal::new_with_timestamp(id, PrincipalRole::User, token, fp, timestamp)
        .expect("Principal creation should succeed");

    assert_eq!(principal.created_at, timestamp);
}

#[test]
fn test_principal_superadmin_bypass_permissions() {
    let id = PrincipalId::new(999);
    let token = SessionToken::new("superadmin-token");
    let fp = CertificateFingerprint::new("sa-fingerprint").unwrap();

    let principal = Principal::new(id, PrincipalRole::SuperAdmin, token, fp)
        .expect("Principal creation should succeed");

    assert!(principal.has_permission(&Permission::AdminShutdown));
    assert!(principal.has_permission(&Permission::AdminRecovery));
    assert!(principal.has_permission(&Permission::AdminCertificateRotate));
    assert!(principal.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
}

#[test]
fn test_disabled_principal_denies_all_role_permissions() {
    let id = PrincipalId::new(1001);
    let token = SessionToken::new("disabled-token");
    let fp = CertificateFingerprint::new(
        "11111111e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
    )
    .unwrap();

    let principal = Principal::new_with_status(
        id,
        PrincipalRole::SuperAdmin,
        PrincipalStatus::Disabled,
        token,
        fp,
    )
    .expect("disabled principal still carries audit identity");

    assert!(!principal.is_active());
    assert!(principal.permissions().is_empty());
    assert!(!principal.has_permission(&Permission::AdminShutdown));
    assert!(!principal.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
}

#[test]
fn test_certificate_identity_requires_sha256_subject_and_surface() {
    let valid = CertificateIdentity::new(
        "22222222e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
        "CN=svc-app",
        SurfaceScope::Application,
    )
    .expect("valid certificate identity");

    assert_eq!(valid.surface_scope(), SurfaceScope::Application);
    assert_eq!(valid.status(), CertificateIdentityStatus::Active);
    assert!(valid.has_identity_evidence());

    let bad_fingerprint =
        CertificateIdentity::new("short", "CN=svc-app", SurfaceScope::Application);
    assert!(bad_fingerprint.is_err());

    let bad_subject = CertificateIdentity::new(
        "33333333e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
        "   ",
        SurfaceScope::Application,
    );
    assert!(bad_subject.is_err());
}

#[test]
fn test_surface_scope_denies_application_crossing_admin_permissions() {
    assert!(
        SurfaceScope::Application
            .permits_permission(&Permission::ExecuteProcedure(ProcedureId::new(42)))
    );
    assert!(SurfaceScope::Application.permits_permission(&Permission::ReadContractMetadata));
    assert!(!SurfaceScope::Application.permits_permission(&Permission::AdminShutdown));
    assert!(SurfaceScope::Administration.permits_permission(&Permission::AdminShutdown));
    assert!(!SurfaceScope::Administration.permits_permission(&Permission::ClusterPromote));
    assert!(!SurfaceScope::Cluster.permits_permission(&Permission::AdminShutdown));
    assert!(SurfaceScope::Cluster.permits_permission(&Permission::ClusterPromote));
}

#[test]
fn test_multiple_principals_independent() {
    let user1 = Principal::new(
        PrincipalId::new(1),
        PrincipalRole::User,
        SessionToken::new("token1"),
        CertificateFingerprint::new("fp1").unwrap(),
    )
    .unwrap();

    let user2 = Principal::new(
        PrincipalId::new(2),
        PrincipalRole::Guest,
        SessionToken::new("token2"),
        CertificateFingerprint::new("fp2").unwrap(),
    )
    .unwrap();

    assert_ne!(user1.id, user2.id);
    assert_ne!(user1.role, user2.role);
    assert_ne!(user1.session_token, user2.session_token);
}

#[test]
fn test_from_certificate_fields_validates_cn() {
    let valid_fp = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

    let result = Principal::from_certificate_fields("mtls-service-001", valid_fp);
    assert!(result.is_ok(), "valid CN must be accepted");

    let result = Principal::from_certificate_fields("", valid_fp);
    assert!(result.is_err(), "empty CN must be rejected");

    let result = Principal::from_certificate_fields("   ", valid_fp);
    assert!(result.is_err(), "whitespace-only CN must be rejected");
}

#[test]
fn test_from_certificate_fields_validates_fingerprint() {
    let valid_cn = "mtls-service-001";

    let valid_fp = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
    let result = Principal::from_certificate_fields(valid_cn, valid_fp);
    assert!(result.is_ok(), "valid SHA-256 fingerprint must be accepted");

    let short_fp = "a1b2c3d4";
    let result = Principal::from_certificate_fields(valid_cn, short_fp);
    assert!(result.is_err(), "short fingerprint must be rejected");

    let non_hex_fp = "g1g2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6g1g2";
    let result = Principal::from_certificate_fields(valid_cn, non_hex_fp);
    assert!(result.is_err(), "non-hex fingerprint must be rejected");

    let result = Principal::from_certificate_fields(valid_cn, "");
    assert!(result.is_err(), "empty fingerprint must be rejected");
}

#[test]
fn test_from_certificate_fields_deterministic() {
    let cn = "mtls-svc-test";
    let fp = "b1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

    let p1 = Principal::from_certificate_fields(cn, fp).expect("first creation");
    let p2 = Principal::from_certificate_fields(cn, fp).expect("second creation");

    assert_eq!(p1.id, p2.id, "same fingerprint → same principal ID");
    assert_eq!(
        p1.session_token, p2.session_token,
        "same fingerprint → same session token"
    );
    assert_eq!(p1.cert_fingerprint, p2.cert_fingerprint);
    assert_eq!(p1.role, p2.role);
}

#[test]
fn test_from_certificate_fields_different_fingerprints_different_ids() {
    let cn = "mtls-svc-test";
    let fp1 = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
    let fp2 = "b2b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

    let p1 = Principal::from_certificate_fields(cn, fp1).expect("first principal");
    let p2 = Principal::from_certificate_fields(cn, fp2).expect("second principal");

    assert_ne!(
        p1.id, p2.id,
        "different fingerprints must produce different principal IDs"
    );
    assert_ne!(
        p1.session_token, p2.session_token,
        "different fingerprints must produce different session tokens"
    );
}

#[test]
fn test_from_certificate_fields_non_zero_id() {
    let cn = "mtls-svc-test";
    let fp = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

    let principal = Principal::from_certificate_fields(cn, fp).expect("principal created");

    assert!(
        !principal.id.is_zero(),
        "certificate-derived principal ID must be non-zero"
    );
}

#[test]
fn test_from_certificate_fields_produces_user_role() {
    let cn = "mtls-svc-test";
    let fp = "c3b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

    let principal = Principal::from_certificate_fields(cn, fp).expect("principal created");

    assert_eq!(
        principal.role,
        PrincipalRole::User,
        "certificate-derived principal must have User role by default"
    );
}

#[test]
fn test_from_certificate_fields_session_token_format() {
    let cn = "mtls-svc-test";
    let fp = "d4b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

    let principal = Principal::from_certificate_fields(cn, fp).expect("principal created");

    assert!(
        principal.session_token.as_str().starts_with("mtls:"),
        "session token must be prefixed with 'mtls:'"
    );
    assert!(
        !principal.session_token.is_empty(),
        "session token must not be empty"
    );
}

#[test]
fn test_from_certificate_fields_permission_evaluation() {
    let cn = "mtls-svc-test";
    let fp = "e5b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

    let principal = Principal::from_certificate_fields(cn, fp).expect("principal created");

    assert!(
        principal.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(u64::MAX))),
        "user role must execute procedures"
    );
    assert!(
        principal.has_permission(&Permission::ReadContractMetadata),
        "user role must read contracts"
    );
    assert!(
        !principal.has_permission(&Permission::AdminShutdown),
        "user role must not shutdown"
    );
}
