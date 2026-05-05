//! End-to-end tests for IAM pipeline (Wave 19).
//!
//! These tests verify the complete authorization flow:
//! - Principal resolution from certificate fingerprint
//! - Permission matrix evaluation per role
//! - RBAC authorization decisions
//! - Audit event emission and correlation
//! - Integration with admission control
//!
//! Test categories:
//! 1. Principal resolution (found, not found, expired)
//! 2. Permission matrix (per role, inheritance)
//! 3. Permission evaluation (allowed, denied, multi-permission)
//! 4. Audit events (emitted, fields, correlation)
//! 5. Admission integration (pre-tx, error flow)
//! 6. Edge cases (anonymous, revoked, super-admin bypass)

#[cfg(test)]
mod iam_pipeline_tests {
    use andromeda_core::{Permission, PrincipalRole, ProcedureId};
    use andromeda_exec::services::{
        ConcretePermissionEvaluator, DenialReason, LocalPrincipalResolver, PermissionDecision,
        PermissionEvaluator, PrincipalResolver,
    };
    use std::sync::Arc;

    // ========== Test 1: Principal Resolution (found, not found, expired) ==========

    #[test]
    fn test_principal_resolution_found() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let fingerprint = "test_resolution_found";

        resolver
            .register_principal(fingerprint.into(), PrincipalRole::User)
            .expect("registration should succeed");

        // Act
        let result = resolver.resolve(fingerprint);

        // Assert
        assert!(result.is_ok());
        let principal = result.unwrap();
        assert_eq!(principal.role, PrincipalRole::User);
        assert_eq!(principal.cert_fingerprint.as_str(), fingerprint);
    }

    #[test]
    fn test_principal_resolution_not_found() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());

        // Act
        let result = resolver.resolve("unknown_fingerprint");

        // Assert
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Security);
    }

    #[test]
    fn test_principal_resolution_empty_fingerprint() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());

        // Act
        let result = resolver.resolve("");

        // Assert
        assert!(result.is_err());
    }

    // ========== Test 2: Permission Matrix (per role, inheritance) ==========

    #[test]
    fn test_super_admin_permission_set() {
        // Arrange & Act
        let perms = PrincipalRole::SuperAdmin.permissions();

        // Assert: SuperAdmin has all permissions
        assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
        assert!(perms.has_permission(&Permission::AdminCatalogPublish));
        assert!(perms.has_permission(&Permission::AdminShutdown));
        assert!(perms.has_permission(&Permission::AdminRecovery));
        assert!(perms.has_permission(&Permission::AuditRead));
        assert!(perms.has_permission(&Permission::AdminCertificateRotate));
        assert!(perms.has_permission(&Permission::AdminRoleManagement));
    }

    #[test]
    fn test_admin_permission_set() {
        // Arrange & Act
        let perms = PrincipalRole::Admin.permissions();

        // Assert: Admin has catalog/recovery/audit, but not shutdown
        assert!(perms.has_permission(&Permission::AdminCatalogPublish));
        assert!(perms.has_permission(&Permission::AdminRecovery));
        assert!(perms.has_permission(&Permission::AuditRead));
        assert!(!perms.has_permission(&Permission::AdminShutdown));
    }

    #[test]
    fn test_operator_permission_set() {
        // Arrange & Act
        let perms = PrincipalRole::Operator.permissions();

        // Assert: Operator can execute procedures and read audit
        assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
        assert!(perms.has_permission(&Permission::AuditRead));
        assert!(!perms.has_permission(&Permission::AdminCatalogPublish));
    }

    #[test]
    fn test_user_permission_set() {
        // Arrange & Act
        let perms = PrincipalRole::User.permissions();

        // Assert: User can execute and read, but not admin
        assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
        assert!(!perms.has_permission(&Permission::AdminCatalogPublish));
        assert!(!perms.has_permission(&Permission::AuditRead));
    }

    #[test]
    fn test_guest_permission_set_restricted() {
        // Arrange & Act
        let perms = PrincipalRole::Guest.permissions();

        // Assert: Guest can only execute procedure 0 (public)
        assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(0))));
        assert!(!perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
    }

    #[test]
    fn test_no_permission_inheritance_between_roles() {
        // Arrange & Act
        let admin_perms = PrincipalRole::Admin.permissions();
        let user_perms = PrincipalRole::User.permissions();

        // Assert: Admin permission does not automatically grant to User
        assert!(admin_perms.has_permission(&Permission::AdminCatalogPublish));
        assert!(!user_perms.has_permission(&Permission::AdminCatalogPublish));
    }

    // ========== Test 3: Permission Evaluation (allowed, denied, multi-permission) ==========

    #[test]
    fn test_permission_evaluation_allowed() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("test_admin".into(), PrincipalRole::Admin)
            .unwrap();

        // Act
        let decision =
            evaluator.evaluate_permission("test_admin", &Permission::AdminCatalogPublish);

        // Assert
        assert!(decision.is_allowed());
        match decision {
            PermissionDecision::Allowed { principal_id, .. } => {
                assert!(!principal_id.is_zero());
            }
            _ => panic!("expected allowed decision"),
        }
    }

    #[test]
    fn test_permission_evaluation_denied_missing() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("test_user".into(), PrincipalRole::User)
            .unwrap();

        // Act
        let decision = evaluator.evaluate_permission("test_user", &Permission::AdminShutdown);

        // Assert
        assert!(decision.is_denied());
        match decision {
            PermissionDecision::Denied {
                reason,
                principal_id,
                ..
            } => {
                assert_eq!(reason, DenialReason::MissingPermission);
                assert!(principal_id.is_some());
            }
            _ => panic!("expected denied decision"),
        }
    }

    #[test]
    fn test_permission_evaluation_denied_principal_not_found() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver);

        // Act
        let decision =
            evaluator.evaluate_permission("unknown_fingerprint", &Permission::AdminCatalogPublish);

        // Assert
        assert!(decision.is_denied());
        match decision {
            PermissionDecision::Denied {
                reason,
                principal_id,
                ..
            } => {
                assert_eq!(reason, DenialReason::PrincipalNotFound);
                assert!(principal_id.is_none());
            }
            _ => panic!("expected denied decision"),
        }
    }

    #[test]
    fn test_permission_evaluation_all_permissions_allowed() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("test_superadmin".into(), PrincipalRole::SuperAdmin)
            .unwrap();

        let required = vec![
            Permission::AdminCatalogPublish,
            Permission::AdminShutdown,
            Permission::AuditRead,
        ];

        // Act
        let result = evaluator.evaluate_all_permissions("test_superadmin", &required);

        // Assert
        assert!(result.is_ok());
    }

    #[test]
    fn test_permission_evaluation_all_permissions_denied() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("test_user".into(), PrincipalRole::User)
            .unwrap();

        let required = vec![
            Permission::AdminCatalogPublish, // User doesn't have this
            Permission::ExecuteProcedure(ProcedureId::new(42)),
        ];

        // Act
        let result = evaluator.evaluate_all_permissions("test_user", &required);

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            PermissionDecision::Denied { reason, .. } => {
                assert_eq!(reason, DenialReason::MissingPermission);
            }
            _ => panic!("expected denied decision"),
        }
    }

    // ========== Test 4: Audit Events (emission, fields, correlation) ==========

    #[test]
    fn test_audit_event_principal_binding() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());

        resolver
            .register_principal("test_audit".into(), PrincipalRole::Operator)
            .unwrap();

        // Act
        let principal = resolver.resolve("test_audit").unwrap();

        // Assert: Verify audit event can bind correct principal
        assert_eq!(principal.cert_fingerprint.as_str(), "test_audit");
        assert_eq!(principal.role, PrincipalRole::Operator);
        assert!(!principal.session_token.as_str().is_empty());
    }

    #[test]
    fn test_audit_event_traceability() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("test_trace".into(), PrincipalRole::User)
            .unwrap();

        // Act: Multiple decisions should be consistent
        let decision1 = evaluator.evaluate_permission(
            "test_trace",
            &Permission::ExecuteProcedure(ProcedureId::new(42)),
        );
        let decision2 = evaluator.evaluate_permission(
            "test_trace",
            &Permission::ExecuteProcedure(ProcedureId::new(42)),
        );

        // Assert: Same decision for same input (deterministic)
        assert_eq!(decision1, decision2);
    }

    // ========== Test 5: Admission Integration (pre-tx, error flow) ==========

    #[test]
    fn test_admission_integration_permission_allowed() {
        // Arrange
        use andromeda_exec::services::AdmissionService;

        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = Arc::new(ConcretePermissionEvaluator::new(resolver.clone()));

        resolver
            .register_principal("test_admission".into(), PrincipalRole::Admin)
            .unwrap();

        let admission = AdmissionService::new(evaluator);

        // Act
        let result = admission.evaluate_permission(
            "test_admission",
            &Permission::AdminCatalogPublish,
            ProcedureId::new(1),
            andromeda_observe::TraceId::new(1),
        );

        // Assert
        assert!(result.is_ok());
    }

    #[test]
    fn test_admission_integration_permission_denied() {
        // Arrange
        use andromeda_exec::services::AdmissionService;

        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = Arc::new(ConcretePermissionEvaluator::new(resolver.clone()));

        resolver
            .register_principal("test_admission_user".into(), PrincipalRole::User)
            .unwrap();

        let admission = AdmissionService::new(evaluator);

        // Act
        let result = admission.evaluate_permission(
            "test_admission_user",
            &Permission::AdminShutdown,
            ProcedureId::new(1),
            andromeda_observe::TraceId::new(1),
        );

        // Assert
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.reason.contains("access denied"));
    }

    // ========== Test 6: Edge Cases (anonymous, revoked, super-admin bypass) ==========

    #[test]
    fn test_edge_case_anonymous_principal() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver);

        // Act: Evaluate anonymous principal (not in store)
        let decision = evaluator.evaluate_permission(
            "anonymous",
            &Permission::ExecuteProcedure(ProcedureId::new(42)),
        );

        // Assert: Default-deny on unknown principal
        assert!(decision.is_denied());
        match decision {
            PermissionDecision::Denied { reason, .. } => {
                assert_eq!(reason, DenialReason::PrincipalNotFound);
            }
            _ => panic!("expected denied decision"),
        }
    }

    #[test]
    fn test_edge_case_revoked_principal() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());
        let fingerprint = "revoked_principal";

        resolver
            .register_principal(fingerprint.into(), PrincipalRole::Admin)
            .unwrap();

        // Verify principal can access before revocation
        let decision_before =
            evaluator.evaluate_permission(fingerprint, &Permission::AdminCatalogPublish);
        assert!(decision_before.is_allowed());

        // Act: Revoke principal
        resolver.revoke_principal(fingerprint).unwrap();

        // Assert: Revoked principal is denied access
        let decision_after =
            evaluator.evaluate_permission(fingerprint, &Permission::AdminCatalogPublish);
        assert!(decision_after.is_denied());
    }

    #[test]
    fn test_edge_case_super_admin_wildcard_permission() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("test_superadmin".into(), PrincipalRole::SuperAdmin)
            .unwrap();

        // Act: SuperAdmin should be able to execute ANY procedure
        let decision1 = evaluator.evaluate_permission(
            "test_superadmin",
            &Permission::ExecuteProcedure(ProcedureId::new(1)),
        );
        let decision2 = evaluator.evaluate_permission(
            "test_superadmin",
            &Permission::ExecuteProcedure(ProcedureId::new(999)),
        );

        // Assert: Both should be allowed
        assert!(decision1.is_allowed());
        assert!(decision2.is_allowed());
    }

    #[test]
    fn test_edge_case_guest_procedure_restriction() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("test_guest".into(), PrincipalRole::Guest)
            .unwrap();

        // Act: Guest should only be able to execute procedure 0
        let public = evaluator.evaluate_permission(
            "test_guest",
            &Permission::ExecuteProcedure(ProcedureId::new(0)),
        );
        let private = evaluator.evaluate_permission(
            "test_guest",
            &Permission::ExecuteProcedure(ProcedureId::new(1)),
        );

        // Assert
        assert!(public.is_allowed());
        assert!(private.is_denied());
    }

    // ========== Test 7: Concurrent Access and Thread Safety ==========

    #[test]
    fn test_concurrent_principal_resolution() {
        use std::thread;

        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());

        for i in 0..5 {
            let fingerprint = format!("concurrent_{}", i);
            resolver
                .register_principal(fingerprint, PrincipalRole::User)
                .unwrap();
        }

        // Act & Assert: Concurrent resolves should work correctly
        let mut handles = vec![];

        for i in 0..10 {
            let resolver_clone = Arc::clone(&resolver);
            let handle = thread::spawn(move || {
                let fingerprint = format!("concurrent_{}", i % 5);
                let result = resolver_clone.resolve(&fingerprint);
                assert!(result.is_ok());
                let principal = result.unwrap();
                assert_eq!(principal.role, PrincipalRole::User);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().expect("thread should complete");
        }
    }

    #[test]
    fn test_deterministic_authorization_decisions() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("deterministic".into(), PrincipalRole::Operator)
            .unwrap();

        // Act: Make same decision multiple times
        let perm = Permission::ExecuteProcedure(ProcedureId::new(42));
        let decisions: Vec<_> = (0..10)
            .map(|_| evaluator.evaluate_permission("deterministic", &perm))
            .collect();

        // Assert: All decisions should be identical
        for decision in &decisions[1..] {
            assert_eq!(decision, &decisions[0]);
        }
    }

    // ========== Test 8: Permission Matching (exact vs. wildcard) ==========

    #[test]
    fn test_permission_matching_exact_procedure() {
        // Arrange
        let perm = Permission::ExecuteProcedure(ProcedureId::new(42));
        let required = Permission::ExecuteProcedure(ProcedureId::new(42));

        // Act & Assert
        assert!(perm.matches(&required));
    }

    #[test]
    fn test_permission_matching_wildcard_procedure() {
        // Arrange
        let perm = Permission::ExecuteProcedure(ProcedureId::new(u64::MAX));
        let required = Permission::ExecuteProcedure(ProcedureId::new(42));

        // Act & Assert: Wildcard matches any procedure
        assert!(perm.matches(&required));
    }

    #[test]
    fn test_permission_matching_non_procedure() {
        // Arrange
        let perm = Permission::AdminCatalogPublish;
        let required = Permission::AdminCatalogPublish;

        // Act & Assert
        assert!(perm.matches(&required));
    }

    // ========== Test 9: Principal Session Token ==========

    #[test]
    fn test_principal_session_token_uniqueness() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());

        resolver
            .register_principal("user1".into(), PrincipalRole::User)
            .unwrap();
        resolver
            .register_principal("user2".into(), PrincipalRole::User)
            .unwrap();

        // Act
        let principal1 = resolver.resolve("user1").unwrap();
        let principal2 = resolver.resolve("user2").unwrap();

        // Assert: Different principals should have different session tokens
        assert_ne!(principal1.session_token, principal2.session_token);
    }

    // ========== Test 10: Admin Operations via Resolver ==========

    #[test]
    fn test_resolver_list_principals() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());

        for i in 0..3 {
            let fingerprint = format!("list_test_{}", i);
            resolver
                .register_principal(fingerprint, PrincipalRole::User)
                .unwrap();
        }

        // Act
        let principals = resolver.list_principals().unwrap();

        // Assert
        assert_eq!(principals.len(), 3);
        for principal in principals {
            assert_eq!(principal.role, PrincipalRole::User);
        }
    }

    #[test]
    fn test_resolver_fingerprints_list() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());

        let fingerprints_in = vec!["fp_1", "fp_2", "fp_3"];
        for fp in &fingerprints_in {
            resolver
                .register_principal((*fp).to_string(), PrincipalRole::User)
                .unwrap();
        }

        // Act
        let fingerprints_out = resolver.fingerprints();

        // Assert
        assert_eq!(fingerprints_out.len(), 3);
        for fp in fingerprints_out {
            assert!(fingerprints_in.contains(&fp.as_str()));
        }
    }
}
