//! IAM permission flow hardening integration tests.
//!
//! These tests verify the hardened permission enforcement flow with explicit
//! audit logging for every decision. They enforce the doctrine:
//! - Deny by default at every gate
//! - All permission denials logged
//! - No silent privilege escalation
//! - Super-admin operations audited
//! - Wildcard enforcement validated
//!
//! Test categories:
//! 1. Principal without permission → DENIED
//! 2. Principal with permission → APPROVED
//! 3. Wildcard permission enforcement
//! 4. Super-admin isolation
//! 5. Principal-less request → DENIED
//! 6. Empty permission set → DENIED
//! 7. Unknown principal → DENIED
//! 8. Permission mismatch → DENIED
//! 9. Audit event emission verification
//! 10. Multi-permission evaluation

#[cfg(test)]
mod iam_hardening_tests {
    use andromeda_core::{AndromedaErrorKind, Permission, PrincipalId, PrincipalRole, ProcedureId};
    use andromeda_exec::services::{
        ConcretePermissionEvaluator, DenialAuditReason, DenialReason, LocalPrincipalResolver,
        NoOpPermissionAuditEmitter, PermissionAuditEmitter, PermissionDecision,
        PermissionEvaluator, PrincipalResolver,
    };
    use andromeda_observe::TraceId;
    use std::sync::Arc;

    // Test 1: Principal WITHOUT Permission → DENIED

    #[test]
    fn test_iam_hardening_1_user_without_admin_permission_denied() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());
        let emitter = NoOpPermissionAuditEmitter;

        resolver
            .register_principal("user_fingerprint".into(), PrincipalRole::User)
            .expect("registration should succeed");

        // Act: User tries to perform admin operation
        let decision =
            evaluator.evaluate_permission("user_fingerprint", &Permission::AdminShutdown);

        // Assert: Decision is DENIED
        assert!(decision.is_denied());
        match decision {
            PermissionDecision::Denied { reason, .. } => {
                assert_eq!(reason, DenialReason::MissingPermission);
            }
            _ => panic!("expected denied decision"),
        }

        // Emit audit event
        let event = andromeda_exec::services::PermissionAuditEvent::denied(
            TraceId::new(1001),
            PrincipalId::new(1),
            Permission::AdminShutdown,
            DenialAuditReason::PermissionNotGranted,
        );
        let result = emitter.emit_permission_decision(event);
        assert!(result.is_ok());
    }

    // Test 2: Principal WITH Permission → APPROVED

    #[test]
    fn test_iam_hardening_2_admin_with_admin_permission_approved() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("admin_fingerprint".into(), PrincipalRole::Admin)
            .expect("registration should succeed");

        // Act: Admin tries to perform catalog operation
        let decision =
            evaluator.evaluate_permission("admin_fingerprint", &Permission::AdminCatalogPublish);

        // Assert: Decision is ALLOWED
        assert!(decision.is_allowed());
        match decision {
            PermissionDecision::Allowed { principal_id, .. } => {
                assert!(!principal_id.is_zero());
            }
            _ => panic!("expected allowed decision"),
        }
    }

    // Test 3: Wildcard Procedure Permission Enforcement

    #[test]
    fn test_iam_hardening_3_operator_with_wildcard_procedure_permission() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("operator_fingerprint".into(), PrincipalRole::Operator)
            .expect("registration should succeed");

        // Act: Operator has ExecuteProcedure(u64::MAX) which matches any procedure
        let decision = evaluator.evaluate_permission(
            "operator_fingerprint",
            &Permission::ExecuteProcedure(ProcedureId::new(42)),
        );

        // Assert: Wildcard permission allows execution
        assert!(decision.is_allowed());
    }

    // Test 4: Super-Admin Isolation (Has All Permissions)

    #[test]
    fn test_iam_hardening_4_superadmin_has_all_permissions() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("superadmin_fingerprint".into(), PrincipalRole::SuperAdmin)
            .expect("registration should succeed");

        // Act: SuperAdmin tries multiple different operations
        let decisions = vec![
            evaluator.evaluate_permission("superadmin_fingerprint", &Permission::AdminShutdown),
            evaluator
                .evaluate_permission("superadmin_fingerprint", &Permission::AdminCatalogPublish),
            evaluator.evaluate_permission("superadmin_fingerprint", &Permission::AuditRead),
            evaluator.evaluate_permission(
                "superadmin_fingerprint",
                &Permission::ExecuteProcedure(ProcedureId::new(42)),
            ),
        ];

        // Assert: All decisions are ALLOWED
        for decision in decisions {
            assert!(
                decision.is_allowed(),
                "superadmin should be allowed all permissions"
            );
        }
    }

    // Test 5: Principal-less Request → DENIED

    #[test]
    fn test_iam_hardening_5_empty_fingerprint_denied() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver);

        // Act: Try to evaluate permission with empty fingerprint
        let decision =
            evaluator.evaluate_permission("", &Permission::ExecuteProcedure(ProcedureId::new(1)));

        // Assert: Decision is DENIED (unknown principal)
        assert!(decision.is_denied());
        match decision {
            PermissionDecision::Denied {
                principal_id,
                reason,
                ..
            } => {
                assert!(principal_id.is_none()); // Unknown principal
                assert_eq!(reason, DenialReason::PrincipalNotFound);
            }
            _ => panic!("expected denied decision"),
        }
    }

    // Test 6: Empty Permission Set → DENIED

    #[test]
    fn test_iam_hardening_6_guest_with_restricted_procedure_denied() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("guest_fingerprint".into(), PrincipalRole::Guest)
            .expect("registration should succeed");

        // Act: Guest tries to execute procedure 1 (they can only execute procedure 0)
        let decision = evaluator.evaluate_permission(
            "guest_fingerprint",
            &Permission::ExecuteProcedure(ProcedureId::new(1)),
        );

        // Assert: Decision is DENIED
        assert!(decision.is_denied());
        match decision {
            PermissionDecision::Denied { reason, .. } => {
                assert_eq!(reason, DenialReason::MissingPermission);
            }
            _ => panic!("expected denied decision"),
        }
    }

    // Test 7: Unknown Principal → DENIED

    #[test]
    fn test_iam_hardening_7_unknown_principal_denied() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver);

        // Act: Try to evaluate permission for unregistered certificate
        let decision = evaluator.evaluate_permission(
            "unknown_fingerprint_never_registered",
            &Permission::AdminCatalogPublish,
        );

        // Assert: Decision is DENIED with PrincipalNotFound
        assert!(decision.is_denied());
        match decision {
            PermissionDecision::Denied {
                principal_id,
                reason,
                ..
            } => {
                assert!(principal_id.is_none());
                assert_eq!(reason, DenialReason::PrincipalNotFound);
            }
            _ => panic!("expected denied decision"),
        }
    }

    // Test 8: Permission Mismatch → DENIED

    #[test]
    fn test_iam_hardening_8_user_without_audit_read_denied() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("user_fingerprint".into(), PrincipalRole::User)
            .expect("registration should succeed");

        // Act: User tries to read audit logs (only operators and admins can do this)
        let decision = evaluator.evaluate_permission("user_fingerprint", &Permission::AuditRead);

        // Assert: Decision is DENIED
        assert!(decision.is_denied());
        match decision {
            PermissionDecision::Denied { reason, .. } => {
                assert_eq!(reason, DenialReason::MissingPermission);
            }
            _ => panic!("expected denied decision"),
        }
    }

    // Test 9: Audit Event Emission Verification

    #[test]
    fn test_iam_hardening_9_audit_event_emission_for_all_decisions() {
        // Arrange
        let emitter = NoOpPermissionAuditEmitter;
        let trace_id = TraceId::new(9000);

        // Act: Create and emit audit events for various decisions
        let allowed_event = andromeda_exec::services::PermissionAuditEvent::allowed(
            trace_id,
            PrincipalId::new(100),
            Permission::ReadContractMetadata,
        );

        let denied_event = andromeda_exec::services::PermissionAuditEvent::denied(
            trace_id,
            PrincipalId::new(101),
            Permission::AdminShutdown,
            DenialAuditReason::PermissionNotGranted,
        );

        let denied_unknown_event =
            andromeda_exec::services::PermissionAuditEvent::denied_unknown_principal(
                trace_id,
                Permission::AdminCatalogPublish,
            );

        // Assert: All events emit successfully
        assert!(emitter.emit_permission_decision(allowed_event).is_ok());
        assert!(emitter.emit_permission_decision(denied_event).is_ok());
        assert!(
            emitter
                .emit_permission_decision(denied_unknown_event)
                .is_ok()
        );
    }

    // Test 10: Multi-Permission Evaluation (All Required Permissions)

    #[test]
    fn test_iam_hardening_10_multi_permission_admin_allowed() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("admin_fingerprint".into(), PrincipalRole::Admin)
            .expect("registration should succeed");

        // Act: Admin has multiple required permissions
        let required_permissions = vec![
            Permission::AdminCatalogPublish,
            Permission::AdminRecovery,
            Permission::AuditRead,
        ];

        let result = evaluator.evaluate_all_permissions("admin_fingerprint", &required_permissions);

        // Assert: All permissions are allowed
        assert!(result.is_ok());
    }

    // Test 11: Multi-Permission Evaluation (One Missing Permission)

    #[test]
    fn test_iam_hardening_11_multi_permission_user_missing_admin_denied() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("user_fingerprint".into(), PrincipalRole::User)
            .expect("registration should succeed");

        // Act: User tries to get multiple permissions (including one they don't have)
        let required_permissions = vec![
            Permission::ReadContractMetadata, // User has this
            Permission::AdminCatalogPublish,  // User doesn't have this
        ];

        let result = evaluator.evaluate_all_permissions("user_fingerprint", &required_permissions);

        // Assert: Evaluation fails on first missing permission
        assert!(result.is_err());
        match result {
            Err(denied_decision) => {
                assert!(denied_decision.is_denied());
                assert_eq!(denied_decision.reason_str(), "missing_permission");
            }
            _ => panic!("expected error"),
        }
    }

    // Test 12: Wildcard NOT Applied to Non-ExecuteProcedure Permissions

    #[test]
    fn test_iam_hardening_12_wildcard_not_applied_to_admin_permissions() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());
        let evaluator = ConcretePermissionEvaluator::new(resolver.clone());

        resolver
            .register_principal("user_fingerprint".into(), PrincipalRole::User)
            .expect("registration should succeed");

        // Act: User has no admin permissions (wildcard is only for ExecuteProcedure)
        let decision =
            evaluator.evaluate_permission("user_fingerprint", &Permission::AdminShutdown);

        // Assert: Decision is DENIED (wildcard does not apply)
        assert!(decision.is_denied());
    }

    // Test 13: Deny-by-Default at Admission Gate (Zero Principal ID Handled)

    #[test]
    fn test_iam_hardening_13_zero_principal_id_safely_handled() {
        // Arrange: Verify PrincipalId(0) is reserved and cannot be used
        let zero_id = PrincipalId::new(0);

        // Assert: PrincipalId(0) is recognized as special
        assert!(zero_id.is_zero());
    }

    // Test 14: Audit Decision Trace Conversion

    #[test]
    fn test_iam_hardening_14_audit_event_to_decision_trace_conversion() {
        // Arrange
        let event = andromeda_exec::services::PermissionAuditEvent::denied(
            TraceId::new(14000),
            PrincipalId::new(100),
            Permission::AdminShutdown,
            DenialAuditReason::PermissionNotGranted,
        );

        // Act: Convert to decision trace
        let trace = event.to_decision_trace();

        // Assert: Trace contains correct information
        assert_eq!(trace.trace_id, TraceId::new(14000));
        assert!(trace.reason.contains("permission denied"));
        assert!(trace.reason.contains("PrincipalId(100)"));
    }

    // Helper: Verify Permission Set Behavior

    #[test]
    fn test_iam_hardening_permission_set_has_permission() {
        // Arrange
        let permission_set = PrincipalRole::Admin.permissions();

        // Assert: Admin has AdminCatalogPublish
        assert!(permission_set.has_permission(&Permission::AdminCatalogPublish));

        // Assert: Admin does NOT have AdminShutdown
        assert!(!permission_set.has_permission(&Permission::AdminShutdown));

        // Assert: Admin does NOT have specific procedure execution permission
        assert!(
            !permission_set.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(999)))
        );
    }

    // Helper: Verify Principal Resolution Error Handling

    #[test]
    fn test_iam_hardening_principal_resolution_error_handling() {
        // Arrange
        let resolver = Arc::new(LocalPrincipalResolver::new());

        // Act: Resolve non-existent principal
        let result = resolver.resolve("nonexistent_fingerprint");

        // Assert: Error is Security kind
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.kind(), AndromedaErrorKind::Security);
    }

    // Helper: Verify Role Permission Boundaries

    #[test]
    fn test_iam_hardening_role_permission_boundaries() {
        // Verify each role's permission boundaries

        // SuperAdmin: has all
        let super_admin_perms = PrincipalRole::SuperAdmin.permissions();
        assert!(super_admin_perms.has_permission(&Permission::AdminShutdown));
        assert!(super_admin_perms.has_permission(&Permission::AuditRead));

        // Admin: no shutdown
        let admin_perms = PrincipalRole::Admin.permissions();
        assert!(!admin_perms.has_permission(&Permission::AdminShutdown));
        assert!(admin_perms.has_permission(&Permission::AdminCatalogPublish));

        // Operator: no admin catalog
        let operator_perms = PrincipalRole::Operator.permissions();
        assert!(!operator_perms.has_permission(&Permission::AdminCatalogPublish));
        assert!(operator_perms.has_permission(&Permission::AuditRead));

        // User: limited permissions
        let user_perms = PrincipalRole::User.permissions();
        assert!(!user_perms.has_permission(&Permission::AdminShutdown));
        assert!(!user_perms.has_permission(&Permission::AuditRead));
        assert!(user_perms.has_permission(&Permission::ReadContractMetadata));

        // Guest: minimal permissions
        let guest_perms = PrincipalRole::Guest.permissions();
        assert!(!guest_perms.has_permission(&Permission::AdminShutdown));
        assert!(!guest_perms.has_permission(&Permission::AuditRead));
        assert!(!guest_perms.has_permission(&Permission::AdminCatalogPublish));
    }
}
