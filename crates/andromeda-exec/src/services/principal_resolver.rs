//! Principal resolution from certificate fingerprint to executor identity.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CertificateFingerprint, Principal,
    PrincipalId, PrincipalRole, SessionToken,
};
use dashmap::DashMap;
use std::sync::Arc;

pub trait PrincipalResolver: Send + Sync {
    fn resolve(&self, cert_fingerprint: &str) -> AndromedaResult<Principal>;

    fn register_principal(
        &self,
        cert_fingerprint: String,
        role: PrincipalRole,
    ) -> AndromedaResult<Principal>;

    fn revoke_principal(&self, cert_fingerprint: &str) -> AndromedaResult<()>;

    fn list_principals(&self) -> AndromedaResult<Vec<Principal>>;
}

pub struct LocalPrincipalResolver {
    principal_store: Arc<DashMap<String, Principal>>,
}

impl LocalPrincipalResolver {
    pub fn new() -> Self {
        Self {
            principal_store: Arc::new(DashMap::new()),
        }
    }

    #[cfg(test)]
    pub fn with_test_principals() -> Self {
        let resolver = Self::new();

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

    pub fn principal_count(&self) -> usize {
        self.principal_store.len()
    }

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
        let fingerprint = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
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
