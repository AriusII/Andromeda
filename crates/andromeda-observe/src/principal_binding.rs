//! V0 principal binding and surface authorization contract.
//!
//! [`PrincipalBinding`] anchors a certificate fingerprint to one
//! [`UserPrincipal`] and a normalized permission set. [`SurfaceAuthorizer`]
//! evaluates that binding against the requested [`SurfaceScope`] and always
//! returns a [`SecurityAuditTrace`] for both allow and deny decisions.

use std::collections::BTreeMap;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::events::{
    AdminOperation, CertificateIdentity, Permission, SecurityAuditOutcome, SecurityAuditTrace,
    SurfaceScope, UserPrincipal,
};
use crate::trace_id::TraceId;

/// The authorized verb a session is attempting before frame dispatch.
///
/// Limited to the procedure-only execution surface plus the explicit
/// administrative operation catalog. There is intentionally no
/// `RawSql`/`AdHocQuery` variant: the doctrine bans an ad hoc SQL native
/// surface, so it is unrepresentable here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceAction {
    /// Invoke a contract-typed procedure on the application surface.
    ExecuteProcedure,
    /// Read a published contract descriptor (no row data, no plan internals).
    ReadContract,
    /// An operator surface command from the [`AdminOperation`] catalog.
    Admin(AdminOperation),
}

impl SurfaceAction {
    /// Returns the [`Permission`] required to authorize this action.
    pub const fn required_permission(self) -> Permission {
        match self {
            Self::ExecuteProcedure => Permission::ExecuteProcedure,
            Self::ReadContract => Permission::ReadContract,
            Self::Admin(op) => op.required_permission(),
        }
    }

    /// Returns true if the action is an operator-surface admin command.
    pub const fn is_admin(self) -> bool {
        matches!(self, Self::Admin(_))
    }

    /// Short evidence label used in audit reasons.
    pub const fn evidence_label(self) -> &'static str {
        match self {
            Self::ExecuteProcedure => "execute_procedure",
            Self::ReadContract => "read_contract",
            Self::Admin(_) => "admin_operation",
        }
    }
}

/// Immutable binding from a certificate identity to a user principal and
/// a closed set of granted permissions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrincipalBinding {
    certificate: CertificateIdentity,
    principal: UserPrincipal,
    permissions: Vec<Permission>,
}

impl PrincipalBinding {
    /// Build a new binding. The permission set is normalized (deduped) so
    /// downstream policy checks are stable regardless of input ordering.
    pub fn new(
        certificate: CertificateIdentity,
        principal: UserPrincipal,
        permissions: Vec<Permission>,
    ) -> AndromedaResult<Self> {
        if !certificate.has_identity_evidence() {
            return Err(security_error(
                "principal binding requires a certificate with identity evidence",
            ));
        }
        if !principal.has_identity_evidence() {
            return Err(security_error(
                "principal binding requires a principal with identity evidence",
            ));
        }
        if certificate.contains_sensitive_evidence() || principal.contains_sensitive_evidence() {
            return Err(security_error(
                "principal binding evidence must not contain secret markers",
            ));
        }

        let mut deduped: Vec<Permission> = Vec::with_capacity(permissions.len());
        for permission in permissions {
            if !deduped.contains(&permission) {
                deduped.push(permission);
            }
        }

        Ok(Self {
            certificate,
            principal,
            permissions: deduped,
        })
    }

    /// Returns the certificate identity backing the binding.
    pub fn certificate(&self) -> &CertificateIdentity {
        &self.certificate
    }

    /// Returns the user principal the certificate maps to.
    pub fn principal(&self) -> &UserPrincipal {
        &self.principal
    }

    /// Returns the set of permissions granted to the principal.
    pub fn permissions(&self) -> &[Permission] {
        &self.permissions
    }

    /// Returns true if the binding grants the requested permission.
    pub fn grants(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }
}

/// In-memory registry of certificate-fingerprint → [`PrincipalBinding`].
#[derive(Debug, Default, Clone)]
pub struct PrincipalRegistry {
    bindings: BTreeMap<String, PrincipalBinding>,
}

impl PrincipalRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            bindings: BTreeMap::new(),
        }
    }

    /// Register a binding, indexed by the certificate fingerprint.
    /// Re-registering the same fingerprint with a different principal id is
    /// rejected to make rotation an explicit, observable workflow.
    pub fn register(&mut self, binding: PrincipalBinding) -> AndromedaResult<()> {
        let key = binding.certificate.fingerprint.clone();
        if let Some(existing) = self.bindings.get(&key)
            && existing.principal.principal_id != binding.principal.principal_id
        {
            return Err(security_error(
                "certificate fingerprint already bound to a different principal id; explicit rotation required",
            ));
        }
        self.bindings.insert(key, binding);
        Ok(())
    }

    /// Number of registered bindings.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Look up a binding by certificate fingerprint.
    pub fn lookup(&self, fingerprint: &str) -> Option<&PrincipalBinding> {
        self.bindings.get(fingerprint)
    }
}

/// Why an authorization decision was reached. Stable, machine-classifiable
/// reasons for both allow and deny outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthorizationDenialReason {
    /// No binding existed for the presented certificate fingerprint.
    UnknownCertificate,
    /// The certificate was issued for a different surface scope than the
    /// surface the request arrived on.
    SurfaceScopeMismatch,
    /// The surface scope itself does not permit the requested permission
    /// (e.g. an admin permission requested on the application surface).
    SurfaceDoesNotPermitPermission,
    /// The principal exists but does not hold the required permission.
    PrincipalMissingPermission,
}

impl AuthorizationDenialReason {
    /// Stable classification label, suitable for audit pipelines.
    pub const fn label(self) -> &'static str {
        match self {
            Self::UnknownCertificate => "unknown_certificate",
            Self::SurfaceScopeMismatch => "surface_scope_mismatch",
            Self::SurfaceDoesNotPermitPermission => "surface_does_not_permit_permission",
            Self::PrincipalMissingPermission => "principal_missing_permission",
        }
    }
}

/// Result of [`SurfaceAuthorizer::authorize`].
///
/// Both branches expose an audit trace: the allow branch records the granted
/// permission for forensic replay; the deny branch records the typed reason
/// so security pipelines can drive rate-limit, lockout, and alerting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationOutcome {
    Allowed {
        principal: UserPrincipal,
        permission: Permission,
        audit: SecurityAuditTrace,
    },
    Denied {
        reason: AuthorizationDenialReason,
        audit: SecurityAuditTrace,
    },
}

impl AuthorizationOutcome {
    /// Returns the audit trace regardless of allow/deny — every security
    /// decision is observable.
    pub fn audit(&self) -> &SecurityAuditTrace {
        match self {
            Self::Allowed { audit, .. } | Self::Denied { audit, .. } => audit,
        }
    }

    /// Convenience predicate for callers that branch on outcome.
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }

    /// Convenience predicate for callers that branch on outcome.
    pub const fn is_denied(&self) -> bool {
        matches!(self, Self::Denied { .. })
    }
}

/// The V0 enforcement boundary: maps an inbound certificate fingerprint to
/// a principal, validates the surface scope and required permission, and
/// always emits a [`SecurityAuditTrace`].
#[derive(Debug, Clone)]
pub struct SurfaceAuthorizer<'a> {
    registry: &'a PrincipalRegistry,
}

impl<'a> SurfaceAuthorizer<'a> {
    /// Wrap an existing registry.
    pub fn new(registry: &'a PrincipalRegistry) -> Self {
        Self { registry }
    }

    /// Authorize a request before dispatch.
    ///
    /// `requested_scope` is the surface the inbound transport asserts (e.g.
    /// the listener-side mapping from a [`SurfacePlane`](crate). `presented_fingerprint`
    /// is the fingerprint observed during the mTLS handshake. `action`
    /// is the dispatch verb the request resolves to.
    pub fn authorize(
        &self,
        trace_id: TraceId,
        requested_scope: SurfaceScope,
        presented_fingerprint: &str,
        action: SurfaceAction,
    ) -> AndromedaResult<AuthorizationOutcome> {
        let permission = action.required_permission();

        let Some(binding) = self.registry.lookup(presented_fingerprint) else {
            // Unknown certificate: emit a deny audit with a synthesized
            // identity envelope so downstream sinks still receive shaped
            // evidence (subject is a stable forensic marker, not the raw
            // certificate body).
            let synth_cert = CertificateIdentity::new(
                fingerprint_for_audit(presented_fingerprint),
                "subject:unknown",
                requested_scope,
            )?;
            let synth_principal = UserPrincipal::new(
                "principal:unknown",
                crate::events::UserPrincipalKind::Service,
            )?;
            let audit = SecurityAuditTrace::new(
                trace_id,
                requested_scope,
                synth_cert,
                synth_principal,
                permission,
                SecurityAuditOutcome::Denied,
                format!(
                    "denied:{}:action={}",
                    AuthorizationDenialReason::UnknownCertificate.label(),
                    action.evidence_label()
                ),
            )?;
            return Ok(AuthorizationOutcome::Denied {
                reason: AuthorizationDenialReason::UnknownCertificate,
                audit,
            });
        };

        // Scope mismatch: the certificate was issued for one surface but
        // is being presented on another. This is a surface-isolation
        // violation regardless of permission grants.
        if binding.certificate.surface != requested_scope {
            let audit = SecurityAuditTrace::new(
                trace_id,
                requested_scope,
                binding.certificate.clone(),
                binding.principal.clone(),
                permission,
                SecurityAuditOutcome::Denied,
                format!(
                    "denied:{}:cert_surface={:?}:requested_surface={:?}:action={}",
                    AuthorizationDenialReason::SurfaceScopeMismatch.label(),
                    binding.certificate.surface,
                    requested_scope,
                    action.evidence_label(),
                ),
            )?;
            return Ok(AuthorizationOutcome::Denied {
                reason: AuthorizationDenialReason::SurfaceScopeMismatch,
                audit,
            });
        }

        // Surface-scope policy: even if the principal has the permission,
        // the surface itself must structurally allow the permission family
        // (e.g. ManageSecurity is never admitted on the Application surface).
        if !requested_scope.permits_permission(permission) {
            let audit = SecurityAuditTrace::new(
                trace_id,
                requested_scope,
                binding.certificate.clone(),
                binding.principal.clone(),
                permission,
                SecurityAuditOutcome::Denied,
                format!(
                    "denied:{}:surface={:?}:permission={:?}:action={}",
                    AuthorizationDenialReason::SurfaceDoesNotPermitPermission.label(),
                    requested_scope,
                    permission,
                    action.evidence_label(),
                ),
            )?;
            return Ok(AuthorizationOutcome::Denied {
                reason: AuthorizationDenialReason::SurfaceDoesNotPermitPermission,
                audit,
            });
        }

        // Principal grant check.
        if !binding.grants(permission) {
            let audit = SecurityAuditTrace::new(
                trace_id,
                requested_scope,
                binding.certificate.clone(),
                binding.principal.clone(),
                permission,
                SecurityAuditOutcome::Denied,
                format!(
                    "denied:{}:permission={:?}:action={}",
                    AuthorizationDenialReason::PrincipalMissingPermission.label(),
                    permission,
                    action.evidence_label(),
                ),
            )?;
            return Ok(AuthorizationOutcome::Denied {
                reason: AuthorizationDenialReason::PrincipalMissingPermission,
                audit,
            });
        }

        // Allow path: still emit an audit so the security pipeline sees a
        // record per dispatch.
        let audit = SecurityAuditTrace::new(
            trace_id,
            requested_scope,
            binding.certificate.clone(),
            binding.principal.clone(),
            permission,
            SecurityAuditOutcome::Allowed,
            format!(
                "allowed:permission={:?}:action={}",
                permission,
                action.evidence_label()
            ),
        )?;

        Ok(AuthorizationOutcome::Allowed {
            principal: binding.principal.clone(),
            permission,
            audit,
        })
    }
}

fn security_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Internal, message)
}

/// Sanitize the presented fingerprint for inclusion in an audit envelope.
/// We never echo an empty string (it would fail evidence validation) and we
/// never echo content that looks like raw key material.
fn fingerprint_for_audit(presented: &str) -> String {
    let trimmed = presented.trim();
    if trimmed.is_empty() {
        return "fingerprint:empty".to_string();
    }
    // Keep the value short and obviously a fingerprint reference.
    let truncated: String = trimmed.chars().take(64).collect();
    format!("fingerprint:{}", truncated)
}

mod bridge;

pub use bridge::{core_principal_to_observe_user_principal, observe_user_principal_to_core};

#[cfg(test)]
mod tests;
