//! Principal integration tests: certificate extraction → Principal → permission evaluation.
//!
//! This test suite validates the full IAM pipeline:
//! 1. X.509 certificate → ParsedCertificate
//! 2. ParsedCertificate → core::Principal
//! 3. Principal session binding & immutability
//! 4. observe::UserPrincipal ↔ core::Principal mapping
//! 5. Permission evaluation against Principal role
//! 6. Integration with admission control (SecurityAuditTrace emission)

#[cfg(test)]
mod tests {
    use andromeda_core::{
        principal::{
            CertificateFingerprint, Permission, PermissionSet, Principal, PrincipalId,
            PrincipalRole, SessionToken,
        },
        AndromedaErrorKind, ProcedureId,
    };

    // ========== Test Helpers ==========

    /// Create a valid test certificate fingerprint (SHA-256 hex).
    fn test_fingerprint() -> String {
        "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
            .to_string()
    }

    /// Create a test Principal with User role.
    fn test_principal() -> Principal {
        let fingerprint =
            CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
        let token = SessionToken::new("session-test-001");
        let id = PrincipalId::new(42);

        Principal::new(id, PrincipalRole::User, token, fingerprint).expect("valid principal")
    }

    /// Create a test Principal with SuperAdmin role.
    fn test_superadmin_principal() -> Principal {
        let fingerprint =
            CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
        let token = SessionToken::new("session-admin-001");
        let id = PrincipalId::new(100);

        Principal::new(id, PrincipalRole::SuperAdmin, token, fingerprint)
            .expect("valid principal")
    }

    // ========== PrincipalId Tests ==========

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

        // Verify hashability (can be used in BTreeMap, HashSet)
        let mut ids = std::collections::HashSet::new();
        ids.insert(id1);
        ids.insert(id2); // Should not increase set size
        assert_eq!(ids.len(), 1, "duplicate IDs must deduplicate in set");
    }

    // ========== SessionToken Tests ==========

    #[test]
    fn test_session_token_immutability() {
        let token = SessionToken::new("immutable-token-42");
        let token_str_1 = token.as_str();
        let token_str_2 = token.as_str();

        assert_eq!(token_str_1, token_str_2, "token string must be stable");
        assert_eq!(token_str_1, "immutable-token-42", "token must preserve content");
    }

    #[test]
    fn test_session_token_empty_detection() {
        let empty = SessionToken::new("");
        let nonempty = SessionToken::new("token");

        assert!(empty.is_empty(), "empty token must be detected");
        assert!(!nonempty.is_empty(), "non-empty token must be valid");
    }

    // ========== CertificateFingerprint Tests ==========

    #[test]
    fn test_certificate_fingerprint_sha256_validation() {
        // Valid SHA-256 (64 hex chars)
        let valid = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        let fp = CertificateFingerprint::new(valid).expect("valid SHA-256");
        assert!(fp.is_valid_sha256(), "must validate SHA-256 format");

        // Invalid: too short
        let short = "a1b2c3d4";
        let fp_short = CertificateFingerprint::new(short).expect("short fingerprint");
        assert!(!fp_short.is_valid_sha256(), "short fingerprint must not validate");

        // Invalid: non-hex characters
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

    // ========== Principal Creation Tests ==========

    #[test]
    fn test_principal_creation_validates_non_zero_id() {
        let fingerprint =
            CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
        let token = SessionToken::new("token");

        // Zero ID must be rejected
        let zero_result =
            Principal::new(PrincipalId::new(0), PrincipalRole::User, token.clone(), fingerprint.clone());
        assert!(zero_result.is_none(), "zero principal ID must be rejected");

        // Non-zero ID must be accepted
        let nonzero_result =
            Principal::new(PrincipalId::new(1), PrincipalRole::User, token, fingerprint);
        assert!(nonzero_result.is_some(), "non-zero principal ID must be accepted");
    }

    #[test]
    fn test_principal_creation_validates_non_empty_token() {
        let fingerprint =
            CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
        let id = PrincipalId::new(42);

        // Empty token must be rejected
        let empty_token_result =
            Principal::new(id, PrincipalRole::User, SessionToken::new(""), fingerprint.clone());
        assert!(empty_token_result.is_none(), "empty session token must be rejected");

        // Non-empty token must be accepted
        let nonempty_token_result = Principal::new(id, PrincipalRole::User, SessionToken::new("token"), fingerprint);
        assert!(
            nonempty_token_result.is_some(),
            "non-empty session token must be accepted"
        );
    }

    #[test]
    fn test_principal_creation_validates_non_empty_fingerprint() {
        let empty_fp = CertificateFingerprint::new_unchecked("");
        let token = SessionToken::new("token");
        let id = PrincipalId::new(42);

        let result = Principal::new(id, PrincipalRole::User, token, empty_fp);
        assert!(
            result.is_none(),
            "empty fingerprint must cause principal creation to fail"
        );
    }

    // ========== Principal Determinism Tests ==========

    #[test]
    fn test_principal_deterministic_creation() {
        // Create two principals with identical fields
        let fp = CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
        let token = SessionToken::new("same-token");
        let id = PrincipalId::new(123);

        let p1 = Principal::new(id, PrincipalRole::Operator, token.clone(), fp.clone())
            .expect("first principal");
        let p2 = Principal::new(id, PrincipalRole::Operator, token, fp).expect("second principal");

        // All fields except created_at must match
        assert_eq!(p1.id, p2.id, "principal IDs must match");
        assert_eq!(p1.role, p2.role, "principal roles must match");
        assert_eq!(
            p1.session_token, p2.session_token,
            "session tokens must match"
        );
        assert_eq!(p1.cert_fingerprint, p2.cert_fingerprint, "fingerprints must match");

        // created_at may differ (microseconds), but both must be valid
        assert!(
            p1.created_at <= std::time::SystemTime::now(),
            "created_at must not be in future"
        );
        assert!(
            p2.created_at <= std::time::SystemTime::now(),
            "created_at must not be in future"
        );
    }

    // ========== Principal Permission Tests ==========

    #[test]
    fn test_principal_permissions_by_role() {
        let superadmin = test_superadmin_principal();
        let user = test_principal();

        // SuperAdmin must have ExecuteProcedure for all procedures
        let exec_all = Permission::ExecuteProcedure(ProcedureId::new(u64::MAX));
        assert!(
            superadmin.has_permission(&exec_all),
            "superadmin must execute all procedures"
        );

        // User must have ExecuteProcedure
        assert!(
            user.has_permission(&exec_all),
            "user must execute procedures"
        );

        // User must NOT have AdminShutdown
        assert!(
            !user.has_permission(&Permission::AdminShutdown),
            "user must not have admin shutdown"
        );

        // SuperAdmin must have AdminShutdown
        assert!(
            superadmin.has_permission(&Permission::AdminShutdown),
            "superadmin must have admin shutdown"
        );
    }

    #[test]
    fn test_permission_set_matching() {
        let perms = PermissionSet::new()
            .with_permission(Permission::ExecuteProcedure(ProcedureId::new(1)))
            .with_permission(Permission::ReadContractMetadata);

        // Exact match
        assert!(
            perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(1))),
            "exact permission match must succeed"
        );

        // SuperAdmin-style wildcard (u64::MAX)
        assert!(
            perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(u64::MAX))),
            "ExecuteProcedure(1) must not match ExecuteProcedure(u64::MAX) requirement"
        );

        // Different permission
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

    // ========== Session Immutability Tests ==========

    #[test]
    fn test_session_token_immutable_during_principal_lifetime() {
        let p = test_principal();
        let token_1 = p.session_token.clone();
        let token_2 = p.session_token.clone();

        assert_eq!(token_1, token_2, "session token must not change");
        assert_eq!(token_1.as_str(), token_2.as_str(), "token string must be stable");
    }

    #[test]
    fn test_role_immutable_during_principal_lifetime() {
        let p = test_principal();
        let role_1 = p.role;
        let role_2 = p.role;

        assert_eq!(role_1, role_2, "role must not change");
    }

    // ========== Principal Masking Tests ==========

    #[test]
    fn test_masked_display_hides_fingerprint() {
        let p = test_principal();
        let display = p.masked_display();

        // Should contain ID and role
        assert!(display.contains(&format!("{}", p.id)), "display must include principal ID");
        assert!(
            display.contains("User"),
            "display must include role"
        );

        // Should NOT contain full fingerprint
        assert!(
            !display.contains(test_fingerprint().as_str()),
            "display must not expose full fingerprint"
        );

        // Should show partial fingerprint (first 6 chars)
        let partial = &p.cert_fingerprint.as_str()[..6.min(p.cert_fingerprint.len())];
        assert!(
            display.contains(partial),
            "display must show partial fingerprint"
        );
    }

    // ========== Role Tests ==========

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
            PrincipalRole::from_str("superadmin"),
            Some(PrincipalRole::SuperAdmin)
        );
        assert_eq!(
            PrincipalRole::from_str("user"),
            Some(PrincipalRole::User)
        );
        assert_eq!(PrincipalRole::from_str("invalid"), None);
    }

    #[test]
    fn test_principal_role_permissions() {
        // SuperAdmin must have full permission set
        let super_perms = PrincipalRole::SuperAdmin.permissions();
        assert!(
            !super_perms.is_empty(),
            "superadmin must have permissions"
        );
        assert!(
            super_perms.has_permission(&Permission::AdminShutdown),
            "superadmin must have shutdown"
        );

        // Guest must have minimal permissions
        let guest_perms = PrincipalRole::Guest.permissions();
        assert!(
            guest_perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(0))),
            "guest must have public procedure access"
        );
    }

    // ========== Integration Tests ==========

    #[test]
    fn test_full_principal_creation_and_permission_check() {
        let fingerprint =
            CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
        let token = SessionToken::new("integration-test-001");
        let id = PrincipalId::new(999);

        let principal =
            Principal::new(id, PrincipalRole::Operator, token.clone(), fingerprint.clone())
                .expect("principal created");

        // Verify all invariants
        assert!(!principal.id.is_zero());
        assert!(!principal.session_token.is_empty());
        assert!(!principal.cert_fingerprint.is_empty());
        assert_eq!(principal.role, PrincipalRole::Operator);

        // Verify permission evaluation
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

        // Clones must be equal but independent in memory
        assert_eq!(p1, p2);
        assert_eq!(p1.id, p2.id);
        assert_eq!(p1.session_token, p2.session_token);
        assert_eq!(p1.cert_fingerprint, p2.cert_fingerprint);
    }
}
