//! Principal resolver: X.509 certificate fingerprint → Principal mapping.
//!
//! This module implements the first stage of the IAM pipeline:
//! - Certificate fingerprint extraction (from DEC-018, mTLS identity)
//! - Fingerprint → Principal lookup in the principal store
//! - Session token validation and binding
//!
//! ## Architecture
//!
//! The resolver uses a DashMap for O(1) concurrent lookups without global locks.
//! Principal bindings are immutable once stored; modifications require explicit updates
//! through the public API.
//!
//! ## Wave 19 Limitations
//!
//! - In-memory store only (no persistence)
//! - No certificate revocation checking
//! - No session expiry or renewal
//! - Flat principal store (no hierarchical organizations)
//!
//! ## Wave 21+ Evolution
//!
//! - Persistent store backed by WAL-replicated table
//! - Certificate revocation list (CRL) checking
//! - Session expiry with renewal tokens
//! - Principal lookup caching with TTL
//! - Role-based scope filtering

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CertificateFingerprint, Principal,
    PrincipalId, PrincipalRole, SessionToken,
};
use dashmap::DashMap;
use std::sync::Arc;

/// PrincipalResolver trait: abstract interface for certificate fingerprint → Principal lookup.
///
/// Implementations may use in-memory stores (Wave 19), persistent databases (Wave 21+),
/// or remote identity services. The trait is Send + Sync for use across async boundaries.
pub trait PrincipalResolver: Send + Sync {
    /// Resolve a certificate fingerprint to a Principal.
    ///
    /// Returns Ok(Principal) if the fingerprint is recognized and maps to an active principal.
    /// Returns Err(PermissionDenied) if the fingerprint is unknown or the principal is revoked.
    ///
    /// # Errors
    ///
    /// Returns `AndromedaErrorKind::Security` for:
    /// - Unknown fingerprint (not in the principal store)
    /// - Expired session token
    /// - Revoked principal (Wave 21+)
    /// - Invalid certificate (malformed or tampered)
    fn resolve(&self, cert_fingerprint: &str) -> AndromedaResult<Principal>;

    /// Register a principal for a certificate fingerprint (admin operation).
    ///
    /// This is called by administrators to bind a certificate to a role.
    /// Subsequent `resolve()` calls with this fingerprint will return the registered Principal.
    ///
    /// # Errors
    ///
    /// Returns `AndromedaErrorKind::Security` if:
    /// - Fingerprint is already registered (duplicate)
    /// - Fingerprint format is invalid
    /// - Caller lacks permission to register principals (Wave 21+)
    fn register_principal(
        &self,
        cert_fingerprint: String,
        role: PrincipalRole,
    ) -> AndromedaResult<Principal>;

    /// Revoke a principal by certificate fingerprint.
    ///
    /// After revocation, `resolve()` calls will fail for this fingerprint.
    /// Existing connections bound to this principal are not immediately closed
    /// (revocation requires reconnection in Wave 19).
    ///
    /// # Errors
    ///
    /// Returns `AndromedaErrorKind::Security` if:
    /// - Fingerprint is not registered
    /// - Caller lacks permission to revoke (Wave 21+)
    fn revoke_principal(&self, cert_fingerprint: &str) -> AndromedaResult<()>;

    /// List all registered principals (for audit and administration).
    fn list_principals(&self) -> AndromedaResult<Vec<Principal>>;
}

/// LocalPrincipalResolver: In-memory principal store using DashMap (Wave 19).
///
/// This implementation stores fingerprint → Principal mappings in a concurrent hash map.
/// It is suitable for development, testing, and short-lived deployments.
///
/// ## Concurrency
///
/// - O(1) lookup via hash table
/// - No global lock (fine-grained locking per bucket)
/// - Safe concurrent register/revoke/resolve operations
///
/// ## Limitations (Wave 19)
///
/// - No persistence across restarts
/// - No certificate revocation checking (CRL/OCSP)
/// - No session expiry
/// - All principals in the same namespace (no multi-tenant isolation)
///
/// ## Wave 21+ Migration Path
///
/// Replace with PersistentPrincipalResolver backed by:
/// - WAL-replicated PrincipalRegistry table
/// - CRL cache with periodic refresh
/// - Session token store with TTL enforcement
pub struct LocalPrincipalResolver {
    principal_store: Arc<DashMap<String, Principal>>,
}

impl LocalPrincipalResolver {
    /// Create a new empty local principal resolver.
    pub fn new() -> Self {
        Self {
            principal_store: Arc::new(DashMap::new()),
        }
    }

    /// Create a resolver pre-populated with test data (for testing and demo).
    ///
    /// This is a convenience method for integration tests. It registers:
    /// - SuperAdmin certificate (for administrative operations)
    /// - Admin certificate (for catalog management)
    /// - Operator certificate (for procedures and audit)
    /// - User certificate (for procedure execution)
    /// - Guest certificate (for read-only access)
    ///
    /// Each certificate has a stable fingerprint for reproducible testing.
    #[cfg(test)]
    pub fn with_test_principals() -> Self {
        let resolver = Self::new();

        // These registrations should not fail in a new resolver
        let _ = resolver.register_principal(
            "test_superadmin_fingerprint".into(),
            PrincipalRole::SuperAdmin,
        );
        let _ = resolver.register_principal("test_admin_fingerprint".into(), PrincipalRole::Admin);
        let _ = resolver
            .register_principal("test_operator_fingerprint".into(), PrincipalRole::Operator);
        let _ = resolver.register_principal("test_user_fingerprint".into(), PrincipalRole::User);
        let _ = resolver.register_principal("test_guest_fingerprint".into(), PrincipalRole::Guest);

        resolver
    }

    /// Get the count of registered principals (for monitoring).
    pub fn principal_count(&self) -> usize {
        self.principal_store.len()
    }

    /// Get all registered fingerprints (for audit).
    pub fn fingerprints(&self) -> Vec<String> {
        self.principal_store
            .iter()
            .map(|ref_multi| ref_multi.key().clone())
            .collect()
    }
}

impl Default for LocalPrincipalResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl PrincipalResolver for LocalPrincipalResolver {
    fn resolve(&self, cert_fingerprint: &str) -> AndromedaResult<Principal> {
        if cert_fingerprint.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "certificate fingerprint must not be empty",
            ));
        }

        self.principal_store
            .get(cert_fingerprint)
            .map(|ref_multi| ref_multi.clone())
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Security,
                    format!(
                        "principal not found: certificate fingerprint '{}' not registered",
                        cert_fingerprint
                    ),
                )
            })
    }

    fn register_principal(
        &self,
        cert_fingerprint: String,
        role: PrincipalRole,
    ) -> AndromedaResult<Principal> {
        if cert_fingerprint.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "certificate fingerprint must not be empty",
            ));
        }

        if self.principal_store.contains_key(&cert_fingerprint) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                format!(
                    "principal registration failed: fingerprint '{}' already registered",
                    cert_fingerprint
                ),
            ));
        }

        let principal_fingerprint = CertificateFingerprint::new(cert_fingerprint.clone())
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Security,
                    "certificate fingerprint must not be empty",
                )
            })?;

        // Centralized pure-core derivation: resolver remains only the store/lookup boundary.
        let principal_id = PrincipalId::from_certificate_fingerprint(&principal_fingerprint)?;
        let session_token = SessionToken::from_certificate_fingerprint(&principal_fingerprint);
        let principal = Principal::new(principal_id, role, session_token, principal_fingerprint)
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Security,
                    "principal registration failed validation",
                )
            })?;

        self.principal_store
            .insert(cert_fingerprint, principal.clone());

        Ok(principal)
    }

    fn revoke_principal(&self, cert_fingerprint: &str) -> AndromedaResult<()> {
        if cert_fingerprint.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "certificate fingerprint must not be empty",
            ));
        }

        self.principal_store
            .remove(cert_fingerprint)
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Security,
                    format!(
                        "principal revocation failed: fingerprint '{}' not found",
                        cert_fingerprint
                    ),
                )
            })?;

        Ok(())
    }

    fn list_principals(&self) -> AndromedaResult<Vec<Principal>> {
        Ok(self
            .principal_store
            .iter()
            .map(|ref_multi| ref_multi.value().clone())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_resolver_creation() {
        let resolver = LocalPrincipalResolver::new();
        assert_eq!(resolver.principal_count(), 0);
    }

    #[test]
    fn test_register_and_resolve_principal() {
        let resolver = LocalPrincipalResolver::new();
        let fingerprint = "test_fingerprint_123";
        let registered = resolver
            .register_principal(fingerprint.into(), PrincipalRole::Admin)
            .expect("registration should succeed");

        assert_eq!(registered.role, PrincipalRole::Admin);
        assert_eq!(registered.cert_fingerprint.as_str(), fingerprint);

        let resolved = resolver
            .resolve(fingerprint)
            .expect("resolve should succeed");

        assert_eq!(resolved, registered);
    }

    #[test]
    fn test_register_uses_core_certificate_derivation_helpers() {
        let resolver = LocalPrincipalResolver::new();
        let fingerprint =
            "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        let certificate = CertificateFingerprint::new(fingerprint).unwrap();

        let registered = resolver
            .register_principal(fingerprint.into(), PrincipalRole::User)
            .expect("registration should succeed");

        assert_eq!(
            registered.id,
            PrincipalId::from_certificate_fingerprint(&certificate).unwrap()
        );
        assert_eq!(
            registered.session_token,
            SessionToken::from_certificate_fingerprint(&certificate)
        );
    }

    #[test]
    fn test_resolve_unknown_principal() {
        let resolver = LocalPrincipalResolver::new();
        let result = resolver.resolve("unknown_fingerprint");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Security);
    }

    #[test]
    fn test_resolve_empty_fingerprint() {
        let resolver = LocalPrincipalResolver::new();
        let result = resolver.resolve("");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Security);
    }

    #[test]
    fn test_register_duplicate_fails() {
        let resolver = LocalPrincipalResolver::new();
        let fingerprint = "duplicate_fingerprint";

        let first_register = resolver.register_principal(fingerprint.into(), PrincipalRole::User);
        assert!(first_register.is_ok());

        let second_register = resolver.register_principal(fingerprint.into(), PrincipalRole::Admin);
        assert!(second_register.is_err());
        assert_eq!(
            second_register.unwrap_err().kind(),
            AndromedaErrorKind::Security
        );
    }

    #[test]
    fn test_revoke_principal() {
        let resolver = LocalPrincipalResolver::new();
        let fingerprint = "revoke_test";

        resolver
            .register_principal(fingerprint.into(), PrincipalRole::User)
            .expect("registration should succeed");

        assert!(resolver.resolve(fingerprint).is_ok());

        resolver
            .revoke_principal(fingerprint)
            .expect("revocation should succeed");

        assert!(resolver.resolve(fingerprint).is_err());
    }

    #[test]
    fn test_revoke_unknown_principal() {
        let resolver = LocalPrincipalResolver::new();
        let result = resolver.revoke_principal("unknown_fingerprint");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Security);
    }

    #[test]
    fn test_list_principals() {
        let resolver = LocalPrincipalResolver::new();

        resolver
            .register_principal("fingerprint_1".into(), PrincipalRole::Admin)
            .unwrap();
        resolver
            .register_principal("fingerprint_2".into(), PrincipalRole::User)
            .unwrap();

        let principals = resolver.list_principals().expect("list should succeed");
        assert_eq!(principals.len(), 2);
    }

    #[test]
    fn test_concurrent_register_and_resolve() {
        use std::thread;

        let resolver = Arc::new(LocalPrincipalResolver::new());
        let mut handles = vec![];

        for i in 0..10 {
            let resolver_clone = Arc::clone(&resolver);
            let handle = thread::spawn(move || {
                let fingerprint = format!("fingerprint_{}", i);
                let role = match i % 5 {
                    0 => PrincipalRole::SuperAdmin,
                    1 => PrincipalRole::Admin,
                    2 => PrincipalRole::Operator,
                    3 => PrincipalRole::User,
                    _ => PrincipalRole::Guest,
                };

                resolver_clone
                    .register_principal(fingerprint, role)
                    .expect("registration should succeed");
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().expect("thread should complete");
        }

        assert_eq!(resolver.principal_count(), 10);
    }

    #[test]
    #[cfg(test)]
    fn test_with_test_principals() {
        let resolver = LocalPrincipalResolver::with_test_principals();
        assert_eq!(resolver.principal_count(), 5);

        let superadmin = resolver
            .resolve("test_superadmin_fingerprint")
            .expect("superadmin should exist");
        assert_eq!(superadmin.role, PrincipalRole::SuperAdmin);

        let guest = resolver
            .resolve("test_guest_fingerprint")
            .expect("guest should exist");
        assert_eq!(guest.role, PrincipalRole::Guest);
    }

    #[test]
    fn test_principal_permissions_preserved() {
        let resolver = LocalPrincipalResolver::new();
        let fingerprint = "permission_test";

        resolver
            .register_principal(fingerprint.into(), PrincipalRole::Operator)
            .expect("registration should succeed");

        let principal = resolver
            .resolve(fingerprint)
            .expect("resolve should succeed");

        // Verify permissions are correctly loaded from role
        assert!(principal.has_permission(&andromeda_core::Permission::AuditRead));
    }

    #[test]
    fn test_fingerprints_list() {
        let resolver = LocalPrincipalResolver::new();

        for i in 0..3 {
            resolver
                .register_principal(format!("fp_{}", i), PrincipalRole::User)
                .unwrap();
        }

        let fingerprints = resolver.fingerprints();
        assert_eq!(fingerprints.len(), 3);
    }
}
