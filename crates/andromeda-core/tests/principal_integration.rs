#[cfg(test)]
mod tests {
    use andromeda_core::{
        CertificateFingerprint, CertificateIdentity, CertificateIdentityStatus, Permission,
        PermissionSet, Principal, PrincipalAuthorizationDenialReason,
        PrincipalAuthorizationEvaluationStage, PrincipalBinding, PrincipalId, PrincipalRegistry,
        PrincipalRole, PrincipalStatus, ProcedureId, SessionToken, SurfaceScope,
    };

    fn test_fingerprint() -> String {
        "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string()
    }

    fn seeded_fingerprint(seed: char) -> String {
        format!("{seed}1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2")
    }

    fn test_principal() -> Principal {
        let fingerprint =
            CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
        let token = SessionToken::new("session-test-001");
        let id = PrincipalId::new(42);

        Principal::new(id, PrincipalRole::User, token, fingerprint).expect("valid principal")
    }

    fn test_superadmin_principal() -> Principal {
        let fingerprint =
            CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
        let token = SessionToken::new("session-admin-001");
        let id = PrincipalId::new(100);

        Principal::new(id, PrincipalRole::SuperAdmin, token, fingerprint).expect("valid principal")
    }

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
        // Valid SHA-256 (64 hex chars)
        let valid = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        let fp = CertificateFingerprint::new(valid).expect("valid SHA-256");
        assert!(fp.is_valid_sha256(), "must validate SHA-256 format");

        // Invalid: too short
        let short = "a1b2c3d4";
        let fp_short = CertificateFingerprint::new(short).expect("short fingerprint");
        assert!(
            !fp_short.is_valid_sha256(),
            "short fingerprint must not validate"
        );

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

    #[test]
    fn test_principal_creation_validates_non_zero_id() {
        let fingerprint =
            CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
        let token = SessionToken::new("token");

        // Zero ID must be rejected
        let zero_result = Principal::new(
            PrincipalId::new(0),
            PrincipalRole::User,
            token.clone(),
            fingerprint.clone(),
        );
        assert!(zero_result.is_none(), "zero principal ID must be rejected");

        // Non-zero ID must be accepted
        let nonzero_result =
            Principal::new(PrincipalId::new(1), PrincipalRole::User, token, fingerprint);
        assert!(
            nonzero_result.is_some(),
            "non-zero principal ID must be accepted"
        );
    }

    #[test]
    fn test_principal_creation_validates_non_empty_token() {
        let fingerprint =
            CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
        let id = PrincipalId::new(42);

        // Empty token must be rejected
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

        // Non-empty token must be accepted
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

        let fingerprint =
            CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
        let token = SessionToken::new("token");
        let id = PrincipalId::new(42);

        assert!(Principal::new(id, PrincipalRole::User, token, fingerprint).is_some());
    }

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
        assert_eq!(
            p1.cert_fingerprint, p2.cert_fingerprint,
            "fingerprints must match"
        );

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
    fn test_disabled_user_principal_is_denied_before_permission_match() {
        let fingerprint = test_fingerprint();
        let certificate = CertificateIdentity::new(
            fingerprint.clone(),
            "CN=svc-disabled",
            SurfaceScope::Application,
        )
        .expect("valid certificate identity");
        let principal = Principal::new_with_status(
            PrincipalId::new(707),
            PrincipalRole::SuperAdmin,
            PrincipalStatus::Disabled,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("disabled principal remains valid audit evidence");
        let binding = PrincipalBinding::new(certificate, principal).expect("valid binding");

        let mut registry = PrincipalRegistry::new();
        registry.register(binding).expect("binding registered");

        let decision = registry.authorize(
            SurfaceScope::Application,
            &fingerprint,
            &Permission::ExecuteProcedure(ProcedureId::new(42)),
        );

        assert!(decision.is_denied());
        assert_eq!(
            decision.denial_reason,
            Some(PrincipalAuthorizationDenialReason::PrincipalDisabled)
        );
        assert!(decision.evidence.has_identity_evidence());
        assert!(decision.evidence.has_reason());
        assert_eq!(decision.evidence.reason, "principal_disabled");
    }

    #[test]
    fn test_certificate_surface_scope_mismatch_denies_before_permission_match() {
        let fingerprint = test_fingerprint();
        let certificate = CertificateIdentity::new(
            fingerprint.clone(),
            "CN=svc-admin",
            SurfaceScope::Administration,
        )
        .expect("valid certificate identity");
        let principal = Principal::new(
            PrincipalId::new(808),
            PrincipalRole::SuperAdmin,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("active principal");
        let binding = PrincipalBinding::new(certificate, principal).expect("valid binding");

        let mut registry = PrincipalRegistry::new();
        registry.register(binding).expect("binding registered");

        let decision = registry.authorize(
            SurfaceScope::Application,
            &fingerprint,
            &Permission::ExecuteProcedure(ProcedureId::new(42)),
        );

        assert!(decision.is_denied());
        assert_eq!(
            decision.denial_reason,
            Some(PrincipalAuthorizationDenialReason::SurfaceScopeMismatch)
        );
        assert_eq!(decision.evidence.surface_scope, SurfaceScope::Application);
        assert_eq!(decision.evidence.reason, "surface_scope_mismatch");
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
    fn test_application_administration_and_cluster_surfaces_are_separate() {
        let mut registry = PrincipalRegistry::new();
        for (fingerprint, surface_scope, principal_id) in [
            (seeded_fingerprint('c'), SurfaceScope::Application, 815),
            (seeded_fingerprint('d'), SurfaceScope::Administration, 816),
            (seeded_fingerprint('e'), SurfaceScope::Cluster, 817),
        ] {
            let certificate =
                CertificateIdentity::new(fingerprint, "CN=svc-surface", surface_scope)
                    .expect("valid certificate identity");
            let principal = Principal::new(
                PrincipalId::new(principal_id),
                PrincipalRole::SuperAdmin,
                SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
                certificate.fingerprint().clone(),
            )
            .expect("active principal");
            registry
                .register(PrincipalBinding::new(certificate, principal).expect("valid binding"))
                .expect("binding registered");
        }

        for (presented_fingerprint, requested_scope, required_permission) in [
            (
                seeded_fingerprint('c'),
                SurfaceScope::Administration,
                Permission::AdminShutdown,
            ),
            (
                seeded_fingerprint('d'),
                SurfaceScope::Cluster,
                Permission::AdminShutdown,
            ),
            (
                seeded_fingerprint('e'),
                SurfaceScope::Application,
                Permission::ExecuteProcedure(ProcedureId::new(42)),
            ),
        ] {
            let decision = registry.authorize(
                requested_scope,
                &presented_fingerprint,
                &required_permission,
            );

            assert!(decision.is_denied());
            assert_eq!(
                decision.denial_reason,
                Some(PrincipalAuthorizationDenialReason::SurfaceScopeMismatch)
            );
            assert_eq!(decision.evidence.surface_scope, requested_scope);
            assert_eq!(decision.evidence.reason, "surface_scope_mismatch");
            assert!(!decision.evidence.role_permission_evaluated);
            assert!(!decision.evidence.direct_permission_evaluated);
        }
    }

    #[test]
    fn test_cluster_surface_rejects_admin_permissions_and_allows_cluster_permissions() {
        let fingerprint = seeded_fingerprint('e');
        let certificate = CertificateIdentity::new(
            fingerprint.clone(),
            "CN=svc-cluster-policy",
            SurfaceScope::Cluster,
        )
        .expect("valid certificate identity");
        let principal = Principal::new(
            PrincipalId::new(821),
            PrincipalRole::SuperAdmin,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("active principal");
        let binding = PrincipalBinding::new(certificate, principal).expect("valid binding");

        assert!(
            !binding.role_grants(&Permission::AdminShutdown),
            "cluster certificates must not carry administration permissions"
        );
        assert!(
            binding.role_grants(&Permission::ClusterPromote),
            "cluster certificates can carry explicit cluster permissions"
        );

        let mut registry = PrincipalRegistry::new();
        registry.register(binding).expect("binding registered");

        let admin_decision = registry.authorize(
            SurfaceScope::Cluster,
            &fingerprint,
            &Permission::AdminShutdown,
        );
        assert!(admin_decision.is_denied());
        assert_eq!(
            admin_decision.denial_reason,
            Some(PrincipalAuthorizationDenialReason::SurfaceDoesNotPermitPermission)
        );
        assert!(!admin_decision.evidence.role_permission_evaluated);

        let cluster_decision = registry.authorize(
            SurfaceScope::Cluster,
            &fingerprint,
            &Permission::ClusterPromote,
        );
        assert!(cluster_decision.is_allowed());
        assert!(cluster_decision.evidence.surface_policy_allowed);
        assert!(cluster_decision.evidence.role_permission_granted);
    }

    #[test]
    fn test_surface_policy_denial_is_observable_before_role_permission_match() {
        let fingerprint = test_fingerprint();
        let certificate = CertificateIdentity::new(
            fingerprint.clone(),
            "CN=svc-application-superadmin",
            SurfaceScope::Application,
        )
        .expect("valid certificate identity");
        let principal = Principal::new(
            PrincipalId::new(809),
            PrincipalRole::SuperAdmin,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("active principal");
        let binding = PrincipalBinding::new(certificate, principal).expect("valid binding");

        let mut registry = PrincipalRegistry::new();
        registry.register(binding).expect("binding registered");

        let decision = registry.authorize(
            SurfaceScope::Application,
            &fingerprint,
            &Permission::AdminShutdown,
        );

        assert!(decision.is_denied());
        assert_eq!(
            decision.denial_reason,
            Some(PrincipalAuthorizationDenialReason::SurfaceDoesNotPermitPermission)
        );
        assert_eq!(
            decision.evidence.certificate_status,
            Some(CertificateIdentityStatus::Active)
        );
        assert_eq!(
            decision.evidence.principal_status,
            Some(PrincipalStatus::Active)
        );
        assert_eq!(
            decision.evidence.certificate_surface_scope,
            Some(SurfaceScope::Application)
        );
        assert!(decision.evidence.surface_policy_evaluated);
        assert!(!decision.evidence.surface_policy_allowed);
        assert!(!decision.evidence.role_permission_evaluated);
        assert!(!decision.evidence.direct_permission_evaluated);
        assert_eq!(
            decision.evidence.reason,
            "surface_does_not_permit_permission"
        );
    }

    #[test]
    fn test_missing_permission_denial_records_role_and_direct_checks() {
        let fingerprint = test_fingerprint();
        let certificate = CertificateIdentity::new(
            fingerprint.clone(),
            "CN=svc-admin-user",
            SurfaceScope::Administration,
        )
        .expect("valid certificate identity");
        let principal = Principal::new(
            PrincipalId::new(810),
            PrincipalRole::User,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("active principal");
        let binding = PrincipalBinding::new(certificate, principal).expect("valid binding");

        let mut registry = PrincipalRegistry::new();
        registry.register(binding).expect("binding registered");

        let decision = registry.authorize(
            SurfaceScope::Administration,
            &fingerprint,
            &Permission::AdminShutdown,
        );

        assert!(decision.is_denied());
        assert_eq!(
            decision.denial_reason,
            Some(PrincipalAuthorizationDenialReason::PrincipalMissingPermission)
        );
        assert!(decision.evidence.surface_policy_evaluated);
        assert!(decision.evidence.surface_policy_allowed);
        assert!(decision.evidence.role_permission_evaluated);
        assert!(!decision.evidence.role_permission_granted);
        assert!(decision.evidence.direct_permission_evaluated);
        assert!(!decision.evidence.direct_permission_granted);
        assert_eq!(decision.evidence.reason, "principal_missing_permission");
    }

    #[test]
    fn test_direct_permission_grant_is_observable_when_role_does_not_grant() {
        let fingerprint = test_fingerprint();
        let certificate = CertificateIdentity::new(
            fingerprint.clone(),
            "CN=svc-admin-direct",
            SurfaceScope::Administration,
        )
        .expect("valid certificate identity");
        let principal = Principal::new(
            PrincipalId::new(811),
            PrincipalRole::User,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("active principal");
        let binding = PrincipalBinding::new_with_direct_permissions(
            certificate,
            principal,
            PermissionSet::new().with_permission(Permission::AdminShutdown),
        )
        .expect("valid binding");

        let mut registry = PrincipalRegistry::new();
        registry.register(binding).expect("binding registered");

        let decision = registry.authorize(
            SurfaceScope::Administration,
            &fingerprint,
            &Permission::AdminShutdown,
        );

        assert!(decision.is_allowed());
        assert!(decision.evidence.surface_policy_allowed);
        assert!(decision.evidence.role_permission_evaluated);
        assert!(!decision.evidence.role_permission_granted);
        assert!(decision.evidence.direct_permission_evaluated);
        assert!(decision.evidence.direct_permission_granted);
    }

    #[test]
    fn test_principal_registry_registration_is_immutable_and_idempotent() {
        let fingerprint = seeded_fingerprint('7');
        let certificate = CertificateIdentity::new(
            fingerprint.clone(),
            "CN=svc-admin-immutable",
            SurfaceScope::Administration,
        )
        .expect("valid certificate identity");
        let principal = Principal::new(
            PrincipalId::new(820),
            PrincipalRole::User,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("active principal");
        let binding =
            PrincipalBinding::new(certificate.clone(), principal.clone()).expect("valid binding");

        let mut registry = PrincipalRegistry::new();
        registry
            .register(binding.clone())
            .expect("initial binding registered");
        registry
            .register(binding)
            .expect("identical binding registration is idempotent");
        assert_eq!(registry.len(), 1);

        let changed_direct_permissions = PrincipalBinding::new_with_direct_permissions(
            certificate.clone(),
            principal.clone(),
            PermissionSet::new().with_permission(Permission::AdminShutdown),
        )
        .expect("direct admin permission is valid for Administration surface");
        assert!(
            registry.register(changed_direct_permissions).is_err(),
            "same certificate fingerprint must not silently mutate direct permissions"
        );

        let changed_role = Principal::new(
            PrincipalId::new(820),
            PrincipalRole::SuperAdmin,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("active principal");
        let changed_role_binding =
            PrincipalBinding::new(certificate, changed_role).expect("valid binding");
        assert!(
            registry.register(changed_role_binding).is_err(),
            "same certificate fingerprint must not silently mutate role evidence"
        );
    }

    #[test]
    fn test_direct_permission_binding_rejects_permission_outside_certificate_surface() {
        let fingerprint = seeded_fingerprint('8');
        let certificate = CertificateIdentity::new(
            fingerprint,
            "CN=svc-application-direct-admin",
            SurfaceScope::Application,
        )
        .expect("valid certificate identity");
        let principal = Principal::new(
            PrincipalId::new(821),
            PrincipalRole::User,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("active principal");

        let binding = PrincipalBinding::new_with_direct_permissions(
            certificate,
            principal,
            PermissionSet::new().with_permission(Permission::AdminShutdown),
        );

        assert!(
            binding.is_err(),
            "Application certificate direct grants must not store Admin permissions"
        );
    }

    #[test]
    fn test_authorization_evidence_reports_stable_evaluation_stage() {
        let unknown = PrincipalRegistry::new().authorize(
            SurfaceScope::Application,
            "unknown-fingerprint",
            &Permission::ExecuteProcedure(ProcedureId::new(42)),
        );
        assert_eq!(
            unknown.evidence.evaluation_stage(),
            PrincipalAuthorizationEvaluationStage::CertificateLookup
        );
        assert!(unknown.evidence.has_policy_version());
        assert!(unknown.evidence.is_audit_ready());

        let fingerprint = seeded_fingerprint('a');
        let certificate = CertificateIdentity::new(
            fingerprint.clone(),
            "CN=svc-stage",
            SurfaceScope::Application,
        )
        .expect("valid certificate identity");
        let principal = Principal::new(
            PrincipalId::new(822),
            PrincipalRole::User,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("active principal");
        let mut registry = PrincipalRegistry::new();
        registry
            .register(PrincipalBinding::new(certificate, principal).expect("valid binding"))
            .expect("binding registered");

        registry
            .revoke_certificate(&fingerprint)
            .expect("certificate revoked");
        let revoked = registry.authorize(
            SurfaceScope::Application,
            &fingerprint,
            &Permission::ExecuteProcedure(ProcedureId::new(42)),
        );
        assert_eq!(
            revoked.evidence.evaluation_stage(),
            PrincipalAuthorizationEvaluationStage::CertificateStatus
        );

        let surface_mismatch_fingerprint = seeded_fingerprint('b');
        let admin_certificate = CertificateIdentity::new(
            surface_mismatch_fingerprint.clone(),
            "CN=svc-stage-admin",
            SurfaceScope::Administration,
        )
        .expect("valid certificate identity");
        let admin_principal = Principal::new(
            PrincipalId::new(823),
            PrincipalRole::SuperAdmin,
            SessionToken::from_certificate_fingerprint(admin_certificate.fingerprint()),
            admin_certificate.fingerprint().clone(),
        )
        .expect("active principal");
        registry
            .register(
                PrincipalBinding::new(admin_certificate, admin_principal).expect("valid binding"),
            )
            .expect("binding registered");
        let surface_mismatch = registry.authorize(
            SurfaceScope::Application,
            &surface_mismatch_fingerprint,
            &Permission::ExecuteProcedure(ProcedureId::new(42)),
        );
        assert_eq!(
            surface_mismatch.evidence.evaluation_stage(),
            PrincipalAuthorizationEvaluationStage::SurfaceScope
        );

        let disabled_fingerprint = seeded_fingerprint('c');
        let disabled_certificate = CertificateIdentity::new(
            disabled_fingerprint.clone(),
            "CN=svc-stage-disabled",
            SurfaceScope::Application,
        )
        .expect("valid certificate identity");
        let disabled_principal = Principal::new_with_status(
            PrincipalId::new(824),
            PrincipalRole::SuperAdmin,
            PrincipalStatus::Disabled,
            SessionToken::from_certificate_fingerprint(disabled_certificate.fingerprint()),
            disabled_certificate.fingerprint().clone(),
        )
        .expect("disabled principal remains valid identity evidence");
        registry
            .register(
                PrincipalBinding::new(disabled_certificate, disabled_principal)
                    .expect("valid binding"),
            )
            .expect("binding registered");
        let disabled = registry.authorize(
            SurfaceScope::Application,
            &disabled_fingerprint,
            &Permission::ExecuteProcedure(ProcedureId::new(42)),
        );
        assert_eq!(
            disabled.evidence.evaluation_stage(),
            PrincipalAuthorizationEvaluationStage::PrincipalStatus
        );

        let surface_policy_fingerprint = seeded_fingerprint('d');
        let app_certificate = CertificateIdentity::new(
            surface_policy_fingerprint.clone(),
            "CN=svc-stage-policy",
            SurfaceScope::Application,
        )
        .expect("valid certificate identity");
        let app_principal = Principal::new(
            PrincipalId::new(825),
            PrincipalRole::SuperAdmin,
            SessionToken::from_certificate_fingerprint(app_certificate.fingerprint()),
            app_certificate.fingerprint().clone(),
        )
        .expect("active principal");
        registry
            .register(PrincipalBinding::new(app_certificate, app_principal).expect("valid binding"))
            .expect("binding registered");
        let surface_policy = registry.authorize(
            SurfaceScope::Application,
            &surface_policy_fingerprint,
            &Permission::AdminShutdown,
        );
        assert_eq!(
            surface_policy.evidence.evaluation_stage(),
            PrincipalAuthorizationEvaluationStage::SurfacePolicy
        );

        let missing_permission_fingerprint = seeded_fingerprint('e');
        let admin_user_certificate = CertificateIdentity::new(
            missing_permission_fingerprint.clone(),
            "CN=svc-stage-missing",
            SurfaceScope::Administration,
        )
        .expect("valid certificate identity");
        let admin_user = Principal::new(
            PrincipalId::new(826),
            PrincipalRole::User,
            SessionToken::from_certificate_fingerprint(admin_user_certificate.fingerprint()),
            admin_user_certificate.fingerprint().clone(),
        )
        .expect("active principal");
        registry
            .register(
                PrincipalBinding::new(admin_user_certificate, admin_user).expect("valid binding"),
            )
            .expect("binding registered");
        let missing_permission = registry.authorize(
            SurfaceScope::Administration,
            &missing_permission_fingerprint,
            &Permission::AdminShutdown,
        );
        assert_eq!(
            missing_permission.evidence.evaluation_stage(),
            PrincipalAuthorizationEvaluationStage::Permission
        );

        let allowed = registry.authorize(
            SurfaceScope::Administration,
            &missing_permission_fingerprint,
            &Permission::ReadContractMetadata,
        );
        assert_eq!(
            allowed.evidence.evaluation_stage(),
            PrincipalAuthorizationEvaluationStage::Allowed
        );
    }

    #[test]
    fn test_role_and_direct_permissions_are_union_after_surface_and_status() {
        let fingerprint = seeded_fingerprint('f');
        let certificate = CertificateIdentity::new(
            fingerprint.clone(),
            "CN=svc-admin-union",
            SurfaceScope::Administration,
        )
        .expect("valid certificate identity");
        let principal = Principal::new(
            PrincipalId::new(818),
            PrincipalRole::User,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("active principal");
        let binding = PrincipalBinding::new_with_direct_permissions(
            certificate,
            principal,
            PermissionSet::new().with_permission(Permission::AdminShutdown),
        )
        .expect("valid binding");

        let mut registry = PrincipalRegistry::new();
        registry.register(binding).expect("binding registered");

        let role_decision = registry.authorize(
            SurfaceScope::Administration,
            &fingerprint,
            &Permission::ReadContractMetadata,
        );
        assert!(role_decision.is_allowed());
        assert!(role_decision.evidence.role_permission_granted);
        assert!(!role_decision.evidence.direct_permission_granted);

        let direct_decision = registry.authorize(
            SurfaceScope::Administration,
            &fingerprint,
            &Permission::AdminShutdown,
        );
        assert!(direct_decision.is_allowed());
        assert!(!direct_decision.evidence.role_permission_granted);
        assert!(direct_decision.evidence.direct_permission_granted);
    }

    #[test]
    fn test_direct_permission_absence_is_a_denial_not_an_implicit_grant() {
        let fingerprint = seeded_fingerprint('9');
        let certificate = CertificateIdentity::new(
            fingerprint.clone(),
            "CN=svc-admin-direct-deny",
            SurfaceScope::Administration,
        )
        .expect("valid certificate identity");
        let principal = Principal::new(
            PrincipalId::new(819),
            PrincipalRole::User,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("active principal");
        let binding = PrincipalBinding::new_with_direct_permissions(
            certificate,
            principal,
            PermissionSet::new().with_permission(Permission::AuditRead),
        )
        .expect("valid binding");

        let mut registry = PrincipalRegistry::new();
        registry.register(binding).expect("binding registered");

        let decision = registry.authorize(
            SurfaceScope::Administration,
            &fingerprint,
            &Permission::AdminShutdown,
        );

        assert!(decision.is_denied());
        assert_eq!(
            decision.denial_reason,
            Some(PrincipalAuthorizationDenialReason::PrincipalMissingPermission)
        );
        assert!(decision.evidence.surface_policy_allowed);
        assert!(decision.evidence.role_permission_evaluated);
        assert!(!decision.evidence.role_permission_granted);
        assert!(decision.evidence.direct_permission_evaluated);
        assert!(!decision.evidence.direct_permission_granted);
    }

    #[test]
    fn test_direct_permissions_do_not_override_disabled_principal() {
        let fingerprint = test_fingerprint();
        let certificate = CertificateIdentity::new(
            fingerprint.clone(),
            "CN=svc-disabled-direct",
            SurfaceScope::Administration,
        )
        .expect("valid certificate identity");
        let principal = Principal::new_with_status(
            PrincipalId::new(812),
            PrincipalRole::User,
            PrincipalStatus::Disabled,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("disabled principal remains audit evidence");
        let binding = PrincipalBinding::new_with_direct_permissions(
            certificate,
            principal,
            PermissionSet::new().with_permission(Permission::AdminShutdown),
        )
        .expect("valid binding");

        assert!(
            !binding.grants(&Permission::AdminShutdown),
            "direct permissions must not bypass disabled principal status"
        );

        let mut registry = PrincipalRegistry::new();
        registry.register(binding).expect("binding registered");

        let decision = registry.authorize(
            SurfaceScope::Administration,
            &fingerprint,
            &Permission::AdminShutdown,
        );

        assert!(decision.is_denied());
        assert_eq!(
            decision.denial_reason,
            Some(PrincipalAuthorizationDenialReason::PrincipalDisabled)
        );
        assert_eq!(
            decision.evidence.principal_status,
            Some(PrincipalStatus::Disabled)
        );
        assert!(!decision.evidence.direct_permission_evaluated);
    }

    #[test]
    fn test_role_permissions_do_not_escape_certificate_surface_scope() {
        let fingerprint = seeded_fingerprint('d');
        let certificate = CertificateIdentity::new(
            fingerprint.clone(),
            "CN=svc-application-superadmin",
            SurfaceScope::Application,
        )
        .expect("valid certificate identity");
        let principal = Principal::new(
            PrincipalId::new(820),
            PrincipalRole::SuperAdmin,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("active principal");
        let binding = PrincipalBinding::new(certificate, principal).expect("valid binding");

        assert!(
            !binding.role_grants(&Permission::AdminShutdown),
            "role permissions must not bypass certificate surface scope"
        );
        assert!(
            !binding.grants(&Permission::AdminShutdown),
            "aggregate grant helpers must remain surface-scoped"
        );

        let mut registry = PrincipalRegistry::new();
        registry.register(binding).expect("binding registered");

        let decision = registry.authorize(
            SurfaceScope::Application,
            &fingerprint,
            &Permission::AdminShutdown,
        );

        assert!(decision.is_denied());
        assert_eq!(
            decision.denial_reason,
            Some(PrincipalAuthorizationDenialReason::SurfaceDoesNotPermitPermission)
        );
        assert!(!decision.evidence.role_permission_evaluated);
        assert!(!decision.evidence.direct_permission_evaluated);
    }

    #[test]
    fn test_certificate_revocation_and_principal_disable_remain_distinct_statuses() {
        let fingerprint = test_fingerprint();
        let certificate = CertificateIdentity::new(
            fingerprint.clone(),
            "CN=svc-status-chain",
            SurfaceScope::Application,
        )
        .expect("valid certificate identity");
        let principal = Principal::new(
            PrincipalId::new(813),
            PrincipalRole::User,
            SessionToken::from_certificate_fingerprint(certificate.fingerprint()),
            certificate.fingerprint().clone(),
        )
        .expect("active principal");

        let mut registry = PrincipalRegistry::new();
        registry
            .register(PrincipalBinding::new(certificate, principal).expect("valid binding"))
            .expect("binding registered");
        registry
            .revoke_certificate(&fingerprint)
            .expect("certificate revoked");

        let revoked_binding = registry.lookup(&fingerprint).expect("binding retained");
        assert_eq!(
            revoked_binding.certificate().status(),
            CertificateIdentityStatus::Revoked
        );
        assert_eq!(revoked_binding.principal().status, PrincipalStatus::Active);

        registry
            .disable_principal(PrincipalId::new(813))
            .expect("principal disabled");

        let disabled_binding = registry.lookup(&fingerprint).expect("binding retained");
        assert_eq!(
            disabled_binding.certificate().status(),
            CertificateIdentityStatus::Revoked
        );
        assert_eq!(
            disabled_binding.principal().status,
            PrincipalStatus::Disabled
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
            !perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(u64::MAX))),
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

        // Should contain ID and role
        assert!(
            display.contains(&format!("{}", p.id)),
            "display must include principal ID"
        );
        assert!(display.contains("User"), "display must include role");

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
        // SuperAdmin must have full permission set
        let super_perms = PrincipalRole::SuperAdmin.permissions();
        assert!(!super_perms.is_empty(), "superadmin must have permissions");
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

    #[test]
    fn test_full_principal_creation_and_permission_check() {
        let fingerprint =
            CertificateFingerprint::new(test_fingerprint()).expect("valid fingerprint");
        let token = SessionToken::new("integration-test-001");
        let id = PrincipalId::new(999);

        let principal = Principal::new(
            id,
            PrincipalRole::Operator,
            token.clone(),
            fingerprint.clone(),
        )
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
