//! Principal identity, roles, and permissions for IAM pipeline (Wave 19).
//!
//! This module defines the identity and access control types for the Andromeda IAM system:
//! - **Principal**: User or service identity bound to an mTLS certificate
//! - **PrincipalRole**: Enumerated role (SuperAdmin, Admin, Operator, User, Guest)
//! - **Permission**: Atomic action (ExecuteProcedure, AdminCatalog, etc.)
//! - **PermissionSet**: Collection of permissions granted to a role
//!
//! ## Authorization Model (RBAC)
//!
//! Authorization follows a three-step flow:
//! 1. **Certificate Extraction** → Principal lookup via certificate fingerprint
//! 2. **Role Binding** → Principal carries one or more PrincipalRoles
//! 3. **Permission Evaluation** → Roles are expanded to permission set; required permission checked
//!
//! ### Default-Deny Posture
//!
//! If a principal holds no matching role, or the role grants no matching permission, access is denied.
//! This is enforced in PermissionSet::evaluate_permission().
//!
//! ## Permission Hierarchy (Wave 19)
//!
//! Permissions are **flat** (no inheritance). Each role explicitly lists its permission set.
//! This design prevents privilege escalation and simplifies audit trail.
//!
//! ## Session Binding (Wave 19)
//!
//! Permissions are **immutable within a session**. Once a principal is bound to a QUIC connection,
//! the permission set is fixed for the connection lifetime. Revocation requires reconnection.
//!
//! ## Wave 21+ Evolution
//!
//! - Persistent principal store (database table + WAL)
//! - Role hierarchy (parent-child roles with transitive grants)
//! - Certificate revocation checking (CRL/OCSP)
//! - Session expiry and renewal
//! - Role-based scope filtering (e.g., Admin manages namespace X only)
//! - Fine-grained procedure-level permissions

use crate::ProcedureId;
use std::collections::HashSet;

/// Unique identifier for a principal (user or service).
///
/// In Wave 19, this is derived from the certificate fingerprint during connection setup.
/// In Wave 21+, this will be persistent and backed by a database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrincipalId(u64);

impl PrincipalId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl From<u64> for PrincipalId {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for PrincipalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PrincipalId({})", self.0)
    }
}

/// Session token bound to a principal for request tracing.
///
/// In Wave 19, this is a per-connection token derived from certificate fingerprint and connection ID.
/// In Wave 21+, this may become a cryptographic token with expiry and refresh semantics.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionToken {
    token: String,
}

impl SessionToken {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.token
    }

    pub fn is_empty(&self) -> bool {
        self.token.is_empty()
    }
}

impl std::fmt::Display for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.token)
    }
}

/// Principal roles in the RBAC model (flat, no inheritance in V0).
///
/// Each role grants a fixed set of permissions. Roles are enumerated at design time;
/// no dynamic role creation in Wave 19.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrincipalRole {
    /// Superuser: all permissions on all operations.
    SuperAdmin,
    /// Administrative operations: catalog management, role management, cluster operations.
    Admin,
    /// Operator: execute procedures, read audit logs, manage backups.
    Operator,
    /// Standard user: execute designated procedures, read contract metadata.
    User,
    /// Guest: minimal access, execute only public procedures.
    Guest,
}

impl PrincipalRole {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::SuperAdmin => "superadmin",
            Self::Admin => "admin",
            Self::Operator => "operator",
            Self::User => "user",
            Self::Guest => "guest",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "superadmin" => Some(Self::SuperAdmin),
            "admin" => Some(Self::Admin),
            "operator" => Some(Self::Operator),
            "user" => Some(Self::User),
            "guest" => Some(Self::Guest),
            _ => None,
        }
    }

    /// Return the permission set granted by this role (Wave 19: flat, no inheritance).
    pub fn permissions(self) -> PermissionSet {
        match self {
            Self::SuperAdmin => PermissionSet::super_admin_permissions(),
            Self::Admin => PermissionSet::admin_permissions(),
            Self::Operator => PermissionSet::operator_permissions(),
            Self::User => PermissionSet::user_permissions(),
            Self::Guest => PermissionSet::guest_permissions(),
        }
    }
}

impl std::fmt::Display for PrincipalRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Atomic permission in the RBAC model (flat, no hierarchy in Wave 19).
///
/// Each permission corresponds to a specific action or capability.
/// Permissions are grouped into scopes (procedure-level, admin-level, read-only).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Permission {
    /// Execute a specific procedure (or all if procedure_id is u64::MAX).
    ExecuteProcedure(ProcedureId),
    /// Read published procedure contracts and metadata.
    ReadContractMetadata,
    /// Create, grant, or revoke roles.
    AdminRoleManagement,
    /// Publish or unpublish procedures in the catalog.
    AdminCatalogPublish,
    /// Stop the server (shutdown operation).
    AdminShutdown,
    /// Trigger restore or recovery operations.
    AdminRecovery,
    /// Query or export audit logs.
    AuditRead,
    /// Rotate or replace mTLS certificates.
    AdminCertificateRotate,
}

impl Permission {
    /// String representation for audit logging and error messages.
    pub fn as_str(&self) -> &str {
        match self {
            Self::ExecuteProcedure(_) => "execute_procedure",
            Self::ReadContractMetadata => "read_contract_metadata",
            Self::AdminRoleManagement => "admin_role_management",
            Self::AdminCatalogPublish => "admin_catalog_publish",
            Self::AdminShutdown => "admin_shutdown",
            Self::AdminRecovery => "admin_recovery",
            Self::AuditRead => "audit_read",
            Self::AdminCertificateRotate => "admin_certificate_rotate",
        }
    }

    /// Check if this permission matches a requirement (Wave 19: exact match for procedure IDs).
    pub fn matches(&self, required: &Permission) -> bool {
        match (self, required) {
            (Self::ExecuteProcedure(granted_id), Self::ExecuteProcedure(required_id)) => {
                // SuperAdmin-like: grant all procedures if granted ID is max value
                granted_id.get() == u64::MAX || granted_id == required_id
            }
            (a, b) => a == b,
        }
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExecuteProcedure(pid) => write!(f, "execute_procedure({})", pid.get()),
            _ => write!(f, "{}", self.as_str()),
        }
    }
}

/// Set of permissions granted to a principal via their assigned roles.
///
/// PermissionSet is immutable once created and evaluated during authorization checks.
/// Wave 19: permissions are flat (no inheritance); each role has an explicit set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionSet {
    permissions: HashSet<Permission>,
}

impl PermissionSet {
    /// Create a new empty permission set.
    pub fn new() -> Self {
        Self {
            permissions: HashSet::new(),
        }
    }

    /// Create a set from a vec of permissions.
    pub fn from_vec(permissions: Vec<Permission>) -> Self {
        Self {
            permissions: permissions.into_iter().collect(),
        }
    }

    /// Add a permission to the set (for role definition).
    pub fn with_permission(mut self, permission: Permission) -> Self {
        self.permissions.insert(permission);
        self
    }

    /// Check if a required permission is held by evaluating all permissions in the set.
    ///
    /// Returns true if any permission in the set matches the required permission.
    /// This implements the RBAC evaluation logic: if any granted permission matches
    /// the required permission, access is allowed. Otherwise, default-deny applies.
    pub fn has_permission(&self, required: &Permission) -> bool {
        self.permissions.iter().any(|p| p.matches(required))
    }

    /// Count of permissions in the set (for auditing and debug).
    pub fn len(&self) -> usize {
        self.permissions.len()
    }

    /// True if the permission set is empty (should be denied access).
    pub fn is_empty(&self) -> bool {
        self.permissions.is_empty()
    }

    /// Iterate over all permissions (for audit logging).
    pub fn iter(&self) -> impl Iterator<Item = &Permission> {
        self.permissions.iter()
    }

    // ========== Role-Specific Permission Sets (Wave 19) ==========

    /// SuperAdmin: all permissions (grant all procedures, all admin operations).
    fn super_admin_permissions() -> Self {
        Self::new()
            .with_permission(Permission::ExecuteProcedure(ProcedureId::new(u64::MAX)))
            .with_permission(Permission::ReadContractMetadata)
            .with_permission(Permission::AdminRoleManagement)
            .with_permission(Permission::AdminCatalogPublish)
            .with_permission(Permission::AdminShutdown)
            .with_permission(Permission::AdminRecovery)
            .with_permission(Permission::AuditRead)
            .with_permission(Permission::AdminCertificateRotate)
    }

    /// Admin: catalog and cluster management, without shutdown.
    fn admin_permissions() -> Self {
        Self::new()
            .with_permission(Permission::ReadContractMetadata)
            .with_permission(Permission::AdminRoleManagement)
            .with_permission(Permission::AdminCatalogPublish)
            .with_permission(Permission::AdminRecovery)
            .with_permission(Permission::AuditRead)
            .with_permission(Permission::AdminCertificateRotate)
    }

    /// Operator: execute procedures, read audit, manage backups.
    fn operator_permissions() -> Self {
        Self::new()
            .with_permission(Permission::ExecuteProcedure(ProcedureId::new(u64::MAX)))
            .with_permission(Permission::ReadContractMetadata)
            .with_permission(Permission::AuditRead)
            .with_permission(Permission::AdminRecovery)
    }

    /// User: execute procedures and read contracts.
    fn user_permissions() -> Self {
        Self::new()
            .with_permission(Permission::ExecuteProcedure(ProcedureId::new(u64::MAX)))
            .with_permission(Permission::ReadContractMetadata)
    }

    /// Guest: minimal access (public procedures only).
    fn guest_permissions() -> Self {
        Self::new().with_permission(Permission::ExecuteProcedure(ProcedureId::new(0)))
    }
}

impl Default for PermissionSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Principal: A user or service identity with roles and permissions.
///
/// A Principal is created when a certificate is successfully extracted and resolved
/// from an mTLS connection. It carries:
/// - `id`: unique identifier (derived from fingerprint in Wave 19)
/// - `role`: assigned role (SuperAdmin, Admin, Operator, User, Guest)
/// - `session_token`: request tracing token
/// - `cert_fingerprint`: certificate fingerprint for revocation checks (Wave 21+)
/// - `created_at`: timestamp of principal creation (for expiry, Wave 21+)
///
/// ## Invariants
///
/// - **Immutable**: once created, a Principal is never modified
/// - **Session-bound**: permissions do not change for the duration of the QUIC connection
/// - **Fail-safe**: unknown principal has no permissions (default-deny)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub id: PrincipalId,
    pub role: PrincipalRole,
    pub session_token: SessionToken,
    pub cert_fingerprint: String,
    pub created_at: std::time::SystemTime,
}

impl Principal {
    /// Create a new principal.
    pub fn new(
        id: PrincipalId,
        role: PrincipalRole,
        session_token: SessionToken,
        cert_fingerprint: String,
    ) -> Self {
        Self {
            id,
            role,
            session_token,
            cert_fingerprint,
            created_at: std::time::SystemTime::now(),
        }
    }

    /// Get the permission set granted to this principal's role.
    pub fn permissions(&self) -> PermissionSet {
        self.role.permissions()
    }

    /// Check if this principal has a specific permission (convenience method).
    pub fn has_permission(&self, required: &Permission) -> bool {
        self.permissions().has_permission(required)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_session_token_creation() {
        let token = SessionToken::new("test-token-123");
        assert_eq!(token.as_str(), "test-token-123");
        assert!(!token.is_empty());
    }

    #[test]
    fn test_principal_role_str_conversion() {
        assert_eq!(PrincipalRole::SuperAdmin.as_str(), "superadmin");
        assert_eq!(PrincipalRole::from_str("admin"), Some(PrincipalRole::Admin));
        assert_eq!(PrincipalRole::from_str("invalid"), None);
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
        // Permission with MAX procedure ID grants all procedures
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
    fn test_permission_set_has_permission() {
        let set = PermissionSet::new()
            .with_permission(Permission::AdminCatalogPublish)
            .with_permission(Permission::AuditRead);

        assert!(set.has_permission(&Permission::AdminCatalogPublish));
        assert!(set.has_permission(&Permission::AuditRead));
        assert!(!set.has_permission(&Permission::AdminShutdown));
    }

    #[test]
    fn test_super_admin_has_all_permissions() {
        let perms = PrincipalRole::SuperAdmin.permissions();
        assert!(perms.has_permission(&Permission::AdminShutdown));
        assert!(perms.has_permission(&Permission::AdminRecovery));
        assert!(perms.has_permission(&Permission::AuditRead));
        assert!(perms.has_permission(&Permission::ExecuteProcedure(
            ProcedureId::new(42)
        )));
    }

    #[test]
    fn test_guest_limited_permissions() {
        let perms = PrincipalRole::Guest.permissions();
        // Guest can only execute procedure ID 0 (public procedures)
        assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(0))));
        // Guest cannot execute other procedures
        assert!(!perms.has_permission(&Permission::ExecuteProcedure(
            ProcedureId::new(42)
        )));
    }

    #[test]
    fn test_operator_permissions() {
        let perms = PrincipalRole::Operator.permissions();
        // Operator can execute any procedure
        assert!(perms.has_permission(&Permission::ExecuteProcedure(
            ProcedureId::new(42)
        )));
        // Operator can read audit
        assert!(perms.has_permission(&Permission::AuditRead));
        // Operator cannot shutdown
        assert!(!perms.has_permission(&Permission::AdminShutdown));
    }

    #[test]
    fn test_principal_creation() {
        let id = PrincipalId::new(1);
        let token = SessionToken::new("test-token");
        let principal = Principal::new(id, PrincipalRole::User, token, "fingerprint-123".into());

        assert_eq!(principal.id, id);
        assert_eq!(principal.role, PrincipalRole::User);
        assert_eq!(principal.cert_fingerprint, "fingerprint-123");
    }

    #[test]
    fn test_principal_has_permission() {
        let principal = Principal::new(
            PrincipalId::new(1),
            PrincipalRole::Admin,
            SessionToken::new("test"),
            "fingerprint".into(),
        );

        assert!(principal.has_permission(&Permission::AdminCatalogPublish));
        assert!(!principal.has_permission(&Permission::AdminShutdown));
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
            Permission::AuditRead, // Duplicate
        ];
        let set = PermissionSet::from_vec(perms);
        assert_eq!(set.len(), 2); // HashSet removes duplicate
    }
}
