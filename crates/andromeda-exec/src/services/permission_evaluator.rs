//! Permission evaluator: RBAC permission checking for authorization decisions.
//!
//! This module implements the second stage of the IAM pipeline:
//! - Principal → permission set lookup
//! - Required permission → evaluation against permission set
//! - Authorization decision (allow/deny with reason)
//!
//! ## Architecture
//!
//! The evaluator holds a reference to a PrincipalResolver for principal lookup
//! and performs deterministic permission matching. Every evaluation is:
//! - **Deterministic**: same input always produces same output
//! - **Fail-safe**: deny on missing principal or permission
//! - **Observable**: returns clear reason for audit logging
//!
//! ## Wave 19 Limitations
//!
//! - Flat permission model (no role hierarchy)
//! - No dynamic permission grants or scope filtering
//! - No attribute-based access control (ABAC)
//! - Permissions fixed at role definition time
//!
//! ## Wave 21+ Evolution
//!
//! - Role hierarchy with transitive permission expansion
//! - Fine-grained scope filtering (namespace-scoped admin permissions)
//! - ABAC with runtime attribute predicates
//! - Permission caching with TTL

use andromeda_core::{AndromedaResult, Permission, Principal, PrincipalId};
use std::sync::Arc;

use super::principal_resolver::PrincipalResolver;

/// Result of a permission evaluation: allow or deny with reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    /// Permission granted; action may proceed.
    Allowed {
        /// Principal that was evaluated.
        principal_id: PrincipalId,
        /// Permission that was granted.
        granted_permission: Permission,
    },
    /// Permission denied; action is blocked.
    Denied {
        /// Principal that was evaluated (if known).
        principal_id: Option<PrincipalId>,
        /// Permission that was required but not held.
        required_permission: Permission,
        /// Machine-readable reason for denial.
        reason: DenialReason,
    },
}

impl PermissionDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Denied { .. })
    }

    pub fn reason_str(&self) -> &str {
        match self {
            Self::Allowed { .. } => "permission_allowed",
            Self::Denied { reason, .. } => reason.as_str(),
        }
    }
}

/// Reason for permission denial (machine-parseable for audit logging).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialReason {
    /// Certificate fingerprint not registered in principal store.
    PrincipalNotFound,
    /// Principal is known but lacks the required permission.
    MissingPermission,
    /// Principal's permission set is empty (default-deny).
    NoPermissionsGranted,
    /// Permission set evaluation logic error (shouldn't happen).
    InternalError,
}

impl DenialReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PrincipalNotFound => "principal_not_found",
            Self::MissingPermission => "missing_permission",
            Self::NoPermissionsGranted => "no_permissions_granted",
            Self::InternalError => "internal_error",
        }
    }
}

impl std::fmt::Display for DenialReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// PermissionEvaluator: RBAC evaluation engine.
///
/// This evaluator checks whether a principal (identified by certificate fingerprint)
/// holds the permissions required for an action. It is the authorization decision point
/// before transaction creation and entry into the execution engine.
///
/// ## Design Invariants
///
/// - **Fail-safe**: Unknown principals are denied access (default-deny)
/// - **Deterministic**: Same principal + permission always produces same decision
/// - **Stateless**: Multiple evaluations are independent
/// - **Observable**: Every decision includes a reason for audit logging
pub trait PermissionEvaluator: Send + Sync {
    fn evaluate_permission(
        &self,
        cert_fingerprint: &str,
        required_permission: &Permission,
    ) -> PermissionDecision;

    fn evaluate_all_permissions(
        &self,
        cert_fingerprint: &str,
        required_permissions: &[Permission],
    ) -> Result<(), PermissionDecision>;

    fn get_principal(&self, cert_fingerprint: &str) -> AndromedaResult<Principal>;
}

pub struct PermissionEvaluatorImpl<R: PrincipalResolver + ?Sized> {
    resolver: Arc<R>,
}

pub type ConcretePermissionEvaluator<R> = PermissionEvaluatorImpl<R>;

impl<R: PrincipalResolver + ?Sized> PermissionEvaluatorImpl<R> {
    /// Create a new permission evaluator with a principal resolver.
    pub fn new(resolver: Arc<R>) -> Self {
        Self { resolver }
    }
}

impl<R: PrincipalResolver + ?Sized> PermissionEvaluator for PermissionEvaluatorImpl<R> {
    /// Evaluate whether a principal has a required permission.
    ///
    /// This is the core authorization decision: given a certificate fingerprint
    /// and a required permission, determine if access should be allowed.
    ///
    /// The decision flow:
    /// 1. Resolve certificate fingerprint to Principal
    /// 2. If resolution fails, deny (default-deny)
    /// 3. Get principal's permission set via role
    /// 4. Check if permission set contains the required permission
    /// 5. Return allow or deny decision
    ///
    /// # Arguments
    ///
    /// * `cert_fingerprint` - X.509 certificate fingerprint (from mTLS)
    /// * `required_permission` - Required permission for the action
    ///
    /// # Returns
    ///
    /// `PermissionDecision` (allow or deny with reason)
    fn evaluate_permission(
        &self,
        cert_fingerprint: &str,
        required_permission: &Permission,
    ) -> PermissionDecision {
        // Step 1: Resolve certificate to principal
        let principal = match self.resolver.resolve(cert_fingerprint) {
            Ok(p) => p,
            Err(_) => {
                // Certificate not in principal store; deny access
                return PermissionDecision::Denied {
                    principal_id: None,
                    required_permission: required_permission.clone(),
                    reason: DenialReason::PrincipalNotFound,
                };
            }
        };

        // Step 2: Get principal's permission set
        let permission_set = principal.permissions();

        // Step 3: Check if permission set is empty (should not happen, but fail-safe)
        if permission_set.is_empty() {
            return PermissionDecision::Denied {
                principal_id: Some(principal.id),
                required_permission: required_permission.clone(),
                reason: DenialReason::NoPermissionsGranted,
            };
        }

        // Step 4: Check if permission set contains required permission
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

    /// Evaluate multiple required permissions (all must be present).
    ///
    /// This is a convenience method for actions requiring multiple permissions.
    /// If any permission is denied, the entire evaluation fails.
    ///
    /// # Arguments
    ///
    /// * `cert_fingerprint` - X.509 certificate fingerprint
    /// * `required_permissions` - Vec of required permissions
    ///
    /// # Returns
    ///
    /// `Ok(())` if all permissions are granted
    /// `Err()` with first denied permission if any permission is denied
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

    /// Get the principal associated with a certificate (for audit logging).
    ///
    /// This is useful for binding audit events to the correct principal.
    fn get_principal(&self, cert_fingerprint: &str) -> AndromedaResult<Principal> {
        self.resolver.resolve(cert_fingerprint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::{
        AndromedaError, AndromedaErrorKind, CertificateFingerprint, PrincipalRole, ProcedureId,
        SessionToken,
    };
    use std::sync::Arc;

    /// Mock principal resolver for testing.
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
            Permission::AdminCatalogPublish, // User doesn't have this
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

        // Operator has ExecuteProcedure(u64::MAX), should match any procedure
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
