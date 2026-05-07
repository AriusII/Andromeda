use std::collections::BTreeMap;

use super::super::{
    CertificateIdentity, CertificateIdentityStatus, Permission, PermissionSet, Principal,
    PrincipalId, PrincipalStatus,
};
use super::security_error;
use crate::AndromedaResult;

/// Immutable registry row that binds one certificate identity to one principal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrincipalBinding {
    certificate: CertificateIdentity,
    principal: Principal,
    direct_permissions: PermissionSet,
}

impl PrincipalBinding {
    pub fn new(certificate: CertificateIdentity, principal: Principal) -> AndromedaResult<Self> {
        Self::new_with_direct_permissions(certificate, principal, PermissionSet::new())
    }

    pub fn new_with_direct_permissions(
        certificate: CertificateIdentity,
        principal: Principal,
        direct_permissions: PermissionSet,
    ) -> AndromedaResult<Self> {
        if !certificate.has_identity_evidence() {
            return Err(security_error(
                "principal binding requires certificate identity evidence",
            ));
        }
        if principal.id.is_zero()
            || principal.session_token.is_empty()
            || principal.cert_fingerprint.is_empty()
        {
            return Err(security_error(
                "principal binding requires principal identity evidence",
            ));
        }
        if principal.cert_fingerprint != *certificate.fingerprint() {
            return Err(security_error(
                "principal binding certificate fingerprint must match principal fingerprint",
            ));
        }
        if direct_permissions
            .iter()
            .any(|permission| !certificate.surface_scope().permits_permission(permission))
        {
            return Err(security_error(
                "principal binding direct permissions must be permitted by certificate surface scope",
            ));
        }

        Ok(Self {
            certificate,
            principal,
            direct_permissions,
        })
    }

    pub fn certificate(&self) -> &CertificateIdentity {
        &self.certificate
    }

    pub fn principal(&self) -> &Principal {
        &self.principal
    }

    pub fn direct_permissions(&self) -> &PermissionSet {
        &self.direct_permissions
    }

    pub fn role_grants(&self, required: &Permission) -> bool {
        self.can_evaluate_permission(required) && self.principal.has_permission(required)
    }

    pub fn direct_grants(&self, required: &Permission) -> bool {
        self.can_evaluate_permission(required) && self.direct_permissions.has_permission(required)
    }

    fn can_evaluate_permission(&self, required: &Permission) -> bool {
        self.certificate.is_active()
            && self.principal.is_active()
            && self
                .certificate
                .surface_scope()
                .permits_permission(required)
    }

    pub fn grants(&self, required: &Permission) -> bool {
        self.role_grants(required) || self.direct_grants(required)
    }

    fn with_certificate_status(&self, status: CertificateIdentityStatus) -> Self {
        Self {
            certificate: self.certificate.with_status(status),
            principal: self.principal.clone(),
            direct_permissions: self.direct_permissions.clone(),
        }
    }

    fn with_principal_status(&self, status: PrincipalStatus) -> Self {
        Self {
            certificate: self.certificate.clone(),
            principal: self.principal.with_status(status),
            direct_permissions: self.direct_permissions.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct PrincipalBindingStore {
    bindings: BTreeMap<String, PrincipalBinding>,
}

impl PrincipalBindingStore {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn register(&mut self, binding: PrincipalBinding) -> AndromedaResult<()> {
        let fingerprint = binding.certificate.fingerprint().as_str().to_string();
        if let Some(existing) = self.bindings.get(&fingerprint) {
            if existing == &binding {
                return Ok(());
            }
            if existing.principal.id != binding.principal.id {
                return Err(security_error(
                    "certificate fingerprint already bound to a different principal id",
                ));
            }
            return Err(security_error(
                "certificate fingerprint already has an immutable principal binding",
            ));
        }

        self.bindings.insert(fingerprint, binding);
        Ok(())
    }

    pub(super) fn lookup(&self, fingerprint: &str) -> Option<&PrincipalBinding> {
        self.bindings.get(fingerprint.trim())
    }

    pub(super) fn len(&self) -> usize {
        self.bindings.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub(super) fn set_certificate_status(
        &mut self,
        fingerprint: &str,
        status: CertificateIdentityStatus,
    ) -> AndromedaResult<()> {
        let key = fingerprint.trim();
        let binding = self
            .bindings
            .get(key)
            .cloned()
            .ok_or_else(|| security_error("certificate identity not found"))?;
        self.bindings
            .insert(key.to_string(), binding.with_certificate_status(status));
        Ok(())
    }

    pub(super) fn disable_principal(&mut self, principal_id: PrincipalId) -> AndromedaResult<()> {
        let mut found = false;
        for binding in self.bindings.values_mut() {
            if binding.principal.id == principal_id {
                *binding = binding.with_principal_status(PrincipalStatus::Disabled);
                found = true;
            }
        }

        if found {
            Ok(())
        } else {
            Err(security_error("principal id not found"))
        }
    }
}
