//! RBAC permission evaluation for executor admission.

use andromeda_core::{AndromedaResult, Principal};
use andromeda_security_contract::PrincipalPermission as Permission;
use std::sync::Arc;

use crate::principal_resolver::PrincipalResolver;

pub use andromeda_admission::{DenialReason, PermissionDecision, PermissionEvaluator};

pub struct PermissionEvaluatorImpl<R: PrincipalResolver + ?Sized> {
    resolver: Arc<R>,
}

pub type ConcretePermissionEvaluator<R> = PermissionEvaluatorImpl<R>;

impl<R: PrincipalResolver + ?Sized> PermissionEvaluatorImpl<R> {
    pub fn new(resolver: Arc<R>) -> Self {
        Self { resolver }
    }
}

impl<R: PrincipalResolver + ?Sized> PermissionEvaluator for PermissionEvaluatorImpl<R> {
    fn evaluate_permission(
        &self,
        cert_fingerprint: &str,
        required_permission: &Permission,
    ) -> PermissionDecision {
        let principal = match self.resolver.resolve(cert_fingerprint) {
            Ok(p) => p,
            Err(_) => {
                return PermissionDecision::Denied {
                    principal_id: None,
                    required_permission: required_permission.clone(),
                    reason: DenialReason::PrincipalNotFound,
                };
            },
        };

        let permission_set = principal.permissions();

        if permission_set.is_empty() {
            return PermissionDecision::Denied {
                principal_id: Some(principal.id),
                required_permission: required_permission.clone(),
                reason: DenialReason::NoPermissionsGranted,
            };
        }

        if permission_set.has_permission(required_permission) {
            PermissionDecision::Allowed {
                principal_id: principal.id,
                granted_permission: required_permission.clone(),
            }
        } else {
            PermissionDecision::Denied {
                principal_id: Some(principal.id),
                required_permission: required_permission.clone(),
                reason: DenialReason::MissingPermission,
            }
        }
    }

    fn evaluate_all_permissions(
        &self,
        cert_fingerprint: &str,
        required_permissions: &[Permission],
    ) -> Result<(), PermissionDecision> {
        for permission in required_permissions {
            match self.evaluate_permission(cert_fingerprint, permission) {
                PermissionDecision::Allowed { .. } => continue,
                denied => return Err(denied),
            }
        }
        Ok(())
    }

    fn get_principal(&self, cert_fingerprint: &str) -> AndromedaResult<Principal> {
        self.resolver.resolve(cert_fingerprint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{
        AndromedaError, AndromedaErrorKind, CertificateFingerprint, PrincipalId, PrincipalRole,
        ProcedureId, SessionToken,
    };
    use std::sync::Arc;

    struct MockResolver {
        principals: std::sync::Arc<std::sync::Mutex<Vec<Principal>>>,
    }

    impl MockResolver {
        fn new() -> Self {
            Self {
                principals: Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }

        fn add_principal(&self, principal: Principal) {
            self.principals.lock().unwrap().push(principal);
        }
    }

    fn principal(role: PrincipalRole, cert_fingerprint: &str) -> Principal {
        Principal::new(
            PrincipalId::new(1),
            role,
            SessionToken::new("test-session"),
            CertificateFingerprint::new(cert_fingerprint)
                .expect("test certificate fingerprint should be valid"),
        )
        .expect("test principal should satisfy core invariants")
    }

    impl PrincipalResolver for MockResolver {
        fn resolve(&self, cert_fingerprint: &str) -> AndromedaResult<Principal> {
            let principals = self.principals.lock().unwrap();
            principals
                .iter()
                .find(|p| p.cert_fingerprint.as_str() == cert_fingerprint)
                .cloned()
                .ok_or_else(|| {
                    AndromedaError::new(AndromedaErrorKind::Security, "principal not found")
                })
        }

        fn register_principal(
            &self,
            cert_fingerprint: String,
            role: PrincipalRole,
        ) -> AndromedaResult<Principal> {
            let principal = principal(role, &cert_fingerprint);
            self.add_principal(principal.clone());
            Ok(principal)
        }

        fn revoke_principal(&self, cert_fingerprint: &str) -> AndromedaResult<()> {
            let mut principals = self.principals.lock().unwrap();
            let position = principals
                .iter()
                .position(|principal| principal.cert_fingerprint.as_str() == cert_fingerprint)
                .ok_or_else(|| {
                    AndromedaError::new(AndromedaErrorKind::Security, "principal not found")
                })?;
            principals.remove(position);
            Ok(())
        }

        fn list_principals(&self) -> AndromedaResult<Vec<Principal>> {
            Ok(self.principals.lock().unwrap().clone())
        }
    }

    #[test]
    fn test_permission_decision_allowed() {
        let decision = PermissionDecision::Allowed {
            principal_id: PrincipalId::new(1),
            granted_permission: Permission::AdminCatalogPublish,
        };

        assert!(decision.is_allowed());
        assert!(!decision.is_denied());
        assert_eq!(decision.reason_str(), "permission_allowed");
    }

    #[test]
    fn test_permission_decision_denied() {
        let decision = PermissionDecision::Denied {
            principal_id: Some(PrincipalId::new(1)),
            required_permission: Permission::AdminShutdown,
            reason: DenialReason::MissingPermission,
        };

        assert!(!decision.is_allowed());
        assert!(decision.is_denied());
        assert_eq!(decision.reason_str(), "missing_permission");
    }

    #[test]
    fn test_denial_reason_str() {
        assert_eq!(
            DenialReason::PrincipalNotFound.as_str(),
            "principal_not_found"
        );
        assert_eq!(
            DenialReason::MissingPermission.as_str(),
            "missing_permission"
        );
        assert_eq!(
            DenialReason::NoPermissionsGranted.as_str(),
            "no_permissions_granted"
        );
    }

    #[test]
    fn test_evaluate_permission_allowed() {
        let resolver = Arc::new(MockResolver::new());
        let evaluator = PermissionEvaluatorImpl::new(resolver.clone());

        resolver.add_principal(principal(PrincipalRole::Admin, "test_admin_fingerprint"));

        let decision = evaluator
            .evaluate_permission("test_admin_fingerprint", &Permission::AdminCatalogPublish);

        assert!(decision.is_allowed());
    }

    #[test]
    fn test_evaluate_permission_denied_missing_permission() {
        let resolver = Arc::new(MockResolver::new());
        let evaluator = PermissionEvaluatorImpl::new(resolver.clone());

        resolver.add_principal(principal(PrincipalRole::User, "test_user_fingerprint"));

        let decision =
            evaluator.evaluate_permission("test_user_fingerprint", &Permission::AdminShutdown);

        assert!(decision.is_denied());
        if let PermissionDecision::Denied { reason, .. } = decision {
            assert_eq!(reason, DenialReason::MissingPermission);
        } else {
            panic!("expected denied decision");
        }
    }

    #[test]
    fn test_evaluate_permission_denied_principal_not_found() {
        let resolver = Arc::new(MockResolver::new());
        let evaluator = PermissionEvaluatorImpl::new(resolver);

        let decision =
            evaluator.evaluate_permission("unknown_fingerprint", &Permission::AdminCatalogPublish);

        assert!(decision.is_denied());
        if let PermissionDecision::Denied {
            reason,
            principal_id,
            ..
        } = decision
        {
            assert_eq!(reason, DenialReason::PrincipalNotFound);
            assert!(principal_id.is_none());
        } else {
            panic!("expected denied decision");
        }
    }

    #[test]
    fn test_evaluate_all_permissions_allowed() {
        let resolver = Arc::new(MockResolver::new());
        let evaluator = PermissionEvaluatorImpl::new(resolver.clone());

        resolver.add_principal(principal(
            PrincipalRole::SuperAdmin,
            "test_superadmin_fingerprint",
        ));

        let required_perms = vec![
            Permission::AdminCatalogPublish,
            Permission::AdminShutdown,
            Permission::AuditRead,
        ];

        let result =
            evaluator.evaluate_all_permissions("test_superadmin_fingerprint", &required_perms);
        assert!(result.is_ok());
    }

    #[test]
    fn test_evaluate_all_permissions_denied() {
        let resolver = Arc::new(MockResolver::new());
        let evaluator = PermissionEvaluatorImpl::new(resolver.clone());

        resolver.add_principal(principal(PrincipalRole::User, "test_user_fingerprint"));

        let required_perms = vec![
            Permission::AdminCatalogPublish,
            Permission::ExecuteProcedure(ProcedureId::new(42)),
        ];

        let result = evaluator.evaluate_all_permissions("test_user_fingerprint", &required_perms);
        assert!(result.is_err());
        if let Err(decision) = result {
            assert!(decision.is_denied());
        }
    }

    #[test]
    fn test_evaluate_execute_procedure_wildcard() {
        let resolver = Arc::new(MockResolver::new());
        let evaluator = PermissionEvaluatorImpl::new(resolver.clone());

        resolver.add_principal(principal(
            PrincipalRole::Operator,
            "test_operator_fingerprint",
        ));

        let decision = evaluator.evaluate_permission(
            "test_operator_fingerprint",
            &Permission::ExecuteProcedure(ProcedureId::new(42)),
        );

        assert!(decision.is_allowed());
    }

    #[test]
    fn test_get_principal() {
        let resolver = Arc::new(MockResolver::new());
        let evaluator = PermissionEvaluatorImpl::new(resolver.clone());

        let principal = principal(PrincipalRole::Admin, "test_admin_fingerprint");

        resolver.add_principal(principal.clone());

        let retrieved = evaluator
            .get_principal("test_admin_fingerprint")
            .expect("should retrieve principal");

        assert_eq!(retrieved, principal);
    }

    #[test]
    fn test_get_principal_not_found() {
        let resolver = Arc::new(MockResolver::new());
        let evaluator = PermissionEvaluatorImpl::new(resolver);

        let result = evaluator.get_principal("unknown_fingerprint");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Security);
    }
}
