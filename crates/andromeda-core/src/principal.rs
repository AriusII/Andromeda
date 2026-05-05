//! Principal identity, roles, and permissions for IAM pipeline (Wave 19+).
//!
//! This module defines the identity and access control types for the Andromeda IAM system:
//! - **PrincipalId**: Unique identifier for user or service identity
//! - **SessionToken**: Per-connection tracing token bound to principal
//! - **CertificateFingerprint**: mTLS certificate identity (SHA-256 hex string)
//! - **PrincipalRole**: Enumerated role (SuperAdmin, Admin, Operator, User, Guest)
//! - **Permission**: Atomic action (ExecuteProcedure, AdminCatalog, etc.)
//! - **PermissionSet**: Collection of permissions granted to a role
//! - **Principal**: Complete user/service identity with role, session, and certificate binding
//!
//! ## Authorization Model (RBAC)
//!
//! Authorization follows a three-step flow:
//! 1. **Certificate Extraction** → Principal lookup via certificate fingerprint
//! 2. **Role Binding** → Principal carries one enumerated PrincipalRole
//! 3. **Permission Evaluation** → Role expanded to permission set; required permission checked
//!
//! ### Default-Deny Posture
//!
//! If a principal holds no matching role, or the role grants no matching permission, access is denied.
//! This is enforced in PermissionSet::has_permission().
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
//! - Duration-based session tokens with expiry
//!
//! ## Type Safety & Validation
//!
//! All principal types enforce validation on creation:
//! - `PrincipalId`: Non-zero values required (0 is reserved for anonymous)
//! - `SessionToken`: Non-empty string required
//! - `CertificateFingerprint`: Valid SHA-256 hex string (64 chars) or freeform string
//! - `Principal`: All constituent fields validated

use crate::ProcedureId;
use std::collections::HashSet;
use std::fmt;

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

/// Certificate fingerprint used for mTLS identity binding.
///
/// In Wave 19, this is a SHA-256 hex string (64 hex characters) extracted from the client certificate.
/// In Wave 21+, this may include validation against CRL/OCSP and certificate expiry checking.
///
/// **Format Examples:**
/// - Valid SHA-256: `"a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7r8s9t0u1v2w3x4y5z6a7b8c9d0e1f2"`
/// - Freeform: Any non-empty string for testing and development
///
/// **Validation Rule:** Non-empty and non-whitespace-only string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CertificateFingerprint(String);

impl CertificateFingerprint {
    /// Create a new certificate fingerprint.
    ///
    /// # Errors
    /// Returns `None` if the fingerprint is empty or contains only whitespace.
    pub fn new(fingerprint: impl Into<String>) -> Option<Self> {
        let fp = fingerprint.into();
        if fp.trim().is_empty() {
            None
        } else {
            Some(Self(fp))
        }
    }

    /// Create a certificate fingerprint without validation (for testing).
    pub fn new_unchecked(fingerprint: impl Into<String>) -> Self {
        Self(fingerprint.into())
    }

    /// Get the fingerprint as a string reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this fingerprint appears to be a valid SHA-256 hex string (64 chars, hex).
    pub fn is_valid_sha256(&self) -> bool {
        if self.0.len() != 64 {
            return false;
        }
        self.0.chars().all(|c| c.is_ascii_hexdigit())
    }

    /// Get the length of the fingerprint string.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if the fingerprint is empty (should not occur after validation).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for CertificateFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for CertificateFingerprint {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for CertificateFingerprint {
    fn from(value: &str) -> Self {
        Self(value.to_string())
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
/// - `id`: unique identifier (non-zero; derived from fingerprint in Wave 19)
/// - `role`: assigned role (SuperAdmin, Admin, Operator, User, Guest)
/// - `session_token`: request tracing token (non-empty)
/// - `cert_fingerprint`: certificate fingerprint for identity and revocation checks (non-empty)
/// - `created_at`: timestamp of principal creation (for expiry, Wave 21+)
///
/// ## Invariants
///
/// - **Unique ID**: Non-zero PrincipalId (0 is reserved for anonymous)
/// - **Non-empty token**: SessionToken must contain at least one character
/// - **Non-empty fingerprint**: CertificateFingerprint must be valid
/// - **Immutable**: Once created, a Principal is never modified
/// - **Session-bound**: Permissions do not change for the duration of the QUIC connection
/// - **Fail-safe**: Unknown principal has no permissions (default-deny)
///
/// ## Creation & Validation
///
/// ```ignore
/// use andromeda_core::Principal;
///
/// // Safe creation with validation
/// let principal = Principal::new(
///     PrincipalId::new(42),
///     PrincipalRole::User,
///     SessionToken::new("session-abc123"),
///     CertificateFingerprint::new("a1b2c3...").unwrap(),
/// );
/// assert!(!principal.id.is_zero());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub id: PrincipalId,
    pub role: PrincipalRole,
    pub session_token: SessionToken,
    pub cert_fingerprint: CertificateFingerprint,
    pub created_at: std::time::SystemTime,
}

impl Principal {
    /// Create a new principal with validation.
    ///
    /// # Requirements
    /// - `id`: Must be non-zero (PrincipalId(0) is reserved for anonymous)
    /// - `session_token`: Must be non-empty
    /// - `cert_fingerprint`: Must be non-empty and valid
    ///
    /// # Returns
    /// `Some(Principal)` if all invariants are satisfied, `None` otherwise.
    pub fn new(
        id: PrincipalId,
        role: PrincipalRole,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
    ) -> Option<Self> {
        // Validation: PrincipalId must be non-zero
        if id.is_zero() {
            return None;
        }

        // Validation: SessionToken must be non-empty
        if session_token.is_empty() {
            return None;
        }

        // Validation: CertificateFingerprint must be non-empty
        if cert_fingerprint.is_empty() {
            return None;
        }

        Some(Self {
            id,
            role,
            session_token,
            cert_fingerprint,
            created_at: std::time::SystemTime::now(),
        })
    }

    /// Create a principal with explicit creation timestamp (for testing).
    pub fn new_with_timestamp(
        id: PrincipalId,
        role: PrincipalRole,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
        created_at: std::time::SystemTime,
    ) -> Option<Self> {
        // Same validation as new()
        if id.is_zero() || session_token.is_empty() || cert_fingerprint.is_empty() {
            return None;
        }

        Some(Self {
            id,
            role,
            session_token,
            cert_fingerprint,
            created_at,
        })
    }

    /// Get the permission set granted to this principal's role.
    pub fn permissions(&self) -> PermissionSet {
        self.role.permissions()
    }

    /// Check if this principal has a specific permission (convenience method).
    pub fn has_permission(&self, required: &Permission) -> bool {
        self.permissions().has_permission(required)
    }

    /// Get a debug-safe representation of this principal (masks sensitive data).
    pub fn masked_display(&self) -> String {
        format!(
            "Principal{{id: {}, role: {}, cert: {}***}}",
            self.id,
            self.role,
            if self.cert_fingerprint.len() > 6 {
                &self.cert_fingerprint.as_str()[..6]
            } else {
                "***"
            }
        )
    }

    /// Create a principal from parsed X.509 certificate fields.
    ///
    /// This is the primary entry point for the mTLS → Principal pipeline:
    /// 1. Extract CN from certificate subject
    /// 2. Validate fingerprint (already done in ParsedCertificate)
    /// 3. Generate SessionToken from fingerprint (deterministic)
    /// 4. Derive PrincipalId from fingerprint (deterministic, non-zero)
    /// 5. Create Principal with User role (role escalation via registry)
    ///
    /// # Invariants
    /// - Result has non-zero PrincipalId
    /// - Result has non-empty SessionToken
    /// - Result has valid CertificateFingerprint
    /// - Deterministic: same certificate → same principal (except created_at timestamp)
    ///
    /// # Arguments
    /// - `subject_cn`: Common Name from certificate Subject (validated non-empty)
    /// - `fingerprint_sha256`: SHA-256 hex string (validated 64 chars, hex)
    ///
    /// # Errors
    /// - `Security` if fingerprint is invalid or PrincipalId derivation fails
    ///
    /// # Example
    /// ```ignore
    /// use andromeda_core::Principal;
    /// use andromeda_quic::ParsedCertificate;
    ///
    /// let cert = ParsedCertificate::new(
    ///     "mtls-service-001".to_string(),
    ///     "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
    ///     None,
    /// )?;
    ///
    /// let principal = Principal::from_certificate_fields(
    ///     &cert.subject_cn,
    ///     &cert.fingerprint_sha256,
    /// )?;
    /// assert!(!principal.id.is_zero());
    /// ```
    pub fn from_certificate_fields(
        subject_cn: &str,
        fingerprint_sha256: &str,
    ) -> crate::AndromedaResult<Self> {
        // 1. Validate CN is non-empty
        let cn = subject_cn.trim();
        if cn.is_empty() {
            return Err(crate::AndromedaError::new(
                crate::AndromedaErrorKind::Security,
                "certificate must have non-empty subject CN",
            ));
        }

        // 2. Create and validate fingerprint
        let fingerprint = CertificateFingerprint::new(fingerprint_sha256).ok_or_else(|| {
            crate::AndromedaError::new(
                crate::AndromedaErrorKind::Security,
                "certificate fingerprint invalid or empty",
            )
        })?;

        // Validate fingerprint is proper SHA-256 format
        if !fingerprint.is_valid_sha256() {
            return Err(crate::AndromedaError::new(
                crate::AndromedaErrorKind::Security,
                "certificate fingerprint must be valid SHA-256 (64 hex chars)",
            ));
        }

        // 3. Generate deterministic SessionToken from fingerprint
        let session_token = Self::session_token_from_fingerprint(fingerprint_sha256);

        // 4. Derive deterministic non-zero PrincipalId from fingerprint
        let principal_id = Self::principal_id_from_fingerprint(fingerprint_sha256)?;

        // 5. Create principal with default User role (escalation via registry)
        Self::new(principal_id, PrincipalRole::User, session_token, fingerprint)
            .ok_or_else(|| {
                crate::AndromedaError::new(
                    crate::AndromedaErrorKind::Security,
                    "principal creation failed: invariant violation",
                )
            })
    }

    /// Derive a stable SessionToken from a certificate fingerprint.
    /// 
    /// Same fingerprint → Same session token (deterministic).
    /// Uses first 32 hex characters + version marker.
    fn session_token_from_fingerprint(fingerprint: &str) -> SessionToken {
        let truncated = if fingerprint.len() >= 32 {
            &fingerprint[..32]
        } else {
            fingerprint
        };
        SessionToken::new(format!("mtls:{}", truncated))
    }

    /// Derive a stable non-zero PrincipalId from certificate fingerprint.
    ///
    /// Parses first 8 hex characters as u64, then ensures non-zero by adding 1 if needed.
    /// Same fingerprint → Same PrincipalId (deterministic).
    fn principal_id_from_fingerprint(fingerprint: &str) -> crate::AndromedaResult<PrincipalId> {
        let hex_part = if fingerprint.len() >= 8 {
            &fingerprint[..8]
        } else {
            fingerprint
        };
        let id_val = u64::from_str_radix(hex_part, 16).map_err(|_| {
            crate::AndromedaError::new(
                crate::AndromedaErrorKind::Security,
                "certificate fingerprint hex parsing failed",
            )
        })?;
        // Ensure non-zero: if first 8 chars parse to 0, use 1 instead
        let final_id = if id_val == 0 { 1 } else { id_val };
        Ok(PrincipalId::new(final_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== PrincipalId Tests ==========

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

    // ========== SessionToken Tests ==========

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

    // ========== CertificateFingerprint Tests ==========

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
    fn test_certificate_fingerprint_valid_sha256() {
        let valid_sha256 = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        let fp = CertificateFingerprint::new(valid_sha256).unwrap();
        assert!(fp.is_valid_sha256());
    }

    #[test]
    fn test_certificate_fingerprint_invalid_sha256_too_short() {
        let short = "a1b2c3d4e5f6"; // Too short
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
    fn test_certificate_fingerprint_from_string() {
        let fp = CertificateFingerprint::from("test".to_string());
        assert_eq!(fp.as_str(), "test");
    }

    #[test]
    fn test_certificate_fingerprint_from_str() {
        let fp = CertificateFingerprint::from("test");
        assert_eq!(fp.as_str(), "test");
    }

    #[test]
    fn test_certificate_fingerprint_unchecked() {
        // Should not fail even with empty string
        let fp = CertificateFingerprint::new_unchecked("");
        assert!(fp.is_empty());
    }

    // ========== PrincipalRole Tests ==========

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
        assert_eq!(PrincipalRole::from_str("superadmin"), Some(PrincipalRole::SuperAdmin));
        assert_eq!(PrincipalRole::from_str("admin"), Some(PrincipalRole::Admin));
        assert_eq!(PrincipalRole::from_str("user"), Some(PrincipalRole::User));
        assert_eq!(PrincipalRole::from_str("invalid"), None);
    }

    #[test]
    fn test_principal_role_display() {
        assert_eq!(PrincipalRole::SuperAdmin.to_string(), "superadmin");
        assert_eq!(PrincipalRole::Operator.to_string(), "operator");
    }

    // ========== Permission Tests ==========

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
    fn test_permission_display() {
        let perm = Permission::ExecuteProcedure(ProcedureId::new(42));
        assert_eq!(perm.to_string(), "execute_procedure(42)");

        let perm2 = Permission::AdminCatalogPublish;
        assert_eq!(perm2.to_string(), "admin_catalog_publish");
    }

    // ========== PermissionSet Tests ==========

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
            Permission::AuditRead, // Duplicate
        ];
        let set = PermissionSet::from_vec(perms);
        assert_eq!(set.len(), 2); // HashSet removes duplicate
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
    fn test_admin_permissions() {
        let perms = PrincipalRole::Admin.permissions();
        // Admin can manage roles
        assert!(perms.has_permission(&Permission::AdminRoleManagement));
        // Admin cannot shutdown
        assert!(!perms.has_permission(&Permission::AdminShutdown));
    }

    #[test]
    fn test_user_permissions() {
        let perms = PrincipalRole::User.permissions();
        // User can execute any procedure
        assert!(perms.has_permission(&Permission::ExecuteProcedure(
            ProcedureId::new(42)
        )));
        // User can read contracts
        assert!(perms.has_permission(&Permission::ReadContractMetadata));
        // User cannot manage roles
        assert!(!perms.has_permission(&Permission::AdminRoleManagement));
    }

    // ========== Principal Tests ==========

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
        assert!(masked.contains("abcdef***")); // First 6 chars + ***
    }

    #[test]
    fn test_principal_with_timestamp() {
        use std::time::{SystemTime, UNIX_EPOCH};

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

        // SuperAdmin can do everything
        assert!(principal.has_permission(&Permission::AdminShutdown));
        assert!(principal.has_permission(&Permission::AdminRecovery));
        assert!(principal.has_permission(&Permission::AdminCertificateRotate));
        assert!(principal.has_permission(&Permission::ExecuteProcedure(
            ProcedureId::new(42)
        )));
    }

    #[test]
    fn test_multiple_principals_independent() {
        let user1 = Principal::new(
            PrincipalId::new(1),
            PrincipalRole::User,
            SessionToken::new("token1"),
            CertificateFingerprint::new("fp1").unwrap(),
        ).unwrap();

        let user2 = Principal::new(
            PrincipalId::new(2),
            PrincipalRole::Guest,
            SessionToken::new("token2"),
            CertificateFingerprint::new("fp2").unwrap(),
        ).unwrap();

        assert_ne!(user1.id, user2.id);
        assert_ne!(user1.role, user2.role);
        assert_ne!(user1.session_token, user2.session_token);
    }

    // ========== Certificate Extraction Tests ==========

    #[test]
    fn test_from_certificate_fields_validates_cn() {
        let valid_fp = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

        // Valid CN
        let result = Principal::from_certificate_fields("mtls-service-001", valid_fp);
        assert!(result.is_ok(), "valid CN must be accepted");

        // Empty CN
        let result = Principal::from_certificate_fields("", valid_fp);
        assert!(result.is_err(), "empty CN must be rejected");

        // Whitespace-only CN
        let result = Principal::from_certificate_fields("   ", valid_fp);
        assert!(result.is_err(), "whitespace-only CN must be rejected");
    }

    #[test]
    fn test_from_certificate_fields_validates_fingerprint() {
        let valid_cn = "mtls-service-001";

        // Valid SHA-256 fingerprint
        let valid_fp = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        let result = Principal::from_certificate_fields(valid_cn, valid_fp);
        assert!(result.is_ok(), "valid SHA-256 fingerprint must be accepted");

        // Invalid: too short
        let short_fp = "a1b2c3d4";
        let result = Principal::from_certificate_fields(valid_cn, short_fp);
        assert!(result.is_err(), "short fingerprint must be rejected");

        // Invalid: non-hex characters
        let non_hex_fp = "g1g2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6g1g2";
        let result = Principal::from_certificate_fields(valid_cn, non_hex_fp);
        assert!(result.is_err(), "non-hex fingerprint must be rejected");

        // Invalid: empty fingerprint
        let result = Principal::from_certificate_fields(valid_cn, "");
        assert!(result.is_err(), "empty fingerprint must be rejected");
    }

    #[test]
    fn test_from_certificate_fields_deterministic() {
        let cn = "mtls-svc-test";
        let fp = "b1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

        let p1 = Principal::from_certificate_fields(cn, fp).expect("first creation");
        let p2 = Principal::from_certificate_fields(cn, fp).expect("second creation");

        // Same certificate fields must produce same principal ID and session token
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

        // Default role must be User (escalation via registry)
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

        // Session token should start with "mtls:" prefix
        assert!(
            principal.session_token.as_str().starts_with("mtls:"),
            "session token must be prefixed with 'mtls:'"
        );

        // Session token should not be empty
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

        // User role must have ExecuteProcedure and ReadContractMetadata
        assert!(
            principal.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(u64::MAX))),
            "user role must execute procedures"
        );
        assert!(
            principal.has_permission(&Permission::ReadContractMetadata),
            "user role must read contracts"
        );

        // User role must NOT have AdminShutdown
        assert!(
            !principal.has_permission(&Permission::AdminShutdown),
            "user role must not shutdown"
        );
    }
}
