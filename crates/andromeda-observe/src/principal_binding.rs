//! V0 principal binding and surface authorization contract.
//!
//! This module wires the existing identity primitives — [`CertificateIdentity`],
//! [`UserPrincipal`], [`Permission`], [`SurfaceScope`] — into a single
//! authorization contract used by listeners and the execution dispatch path
//! to enforce **mTLS → principal → permissions → policy → audit** before a
//! procedure or admin operation is dispatched.
//!
//! ## V0 contract
//!
//! 1. A [`PrincipalBinding`] anchors a certificate fingerprint to a single
//!    [`UserPrincipal`] and a closed set of granted [`Permission`]s.
//! 2. A [`PrincipalRegistry`] indexes bindings by certificate fingerprint.
//!    The registry is intentionally in-memory: real key material loading,
//!    rotation, and revocation are out of scope for the V0 scaffold.
//! 3. A [`SurfaceAuthorizer`] decides whether an inbound action on a given
//!    [`SurfaceScope`] is permitted for the certificate that opened the
//!    session, returning an [`AuthorizationOutcome`] that **always** carries
//!    a [`SecurityAuditTrace`] — both allow and deny paths are observable and
//!    auditable, so security decisions cannot disappear silently.
//!
//! ## No-go alignment
//!
//! - No SQL ad hoc surface is admitted: [`SurfaceAction`] only models
//!   procedure execution, contract reads, and the explicit
//!   [`AdminOperation`] catalog.
//! - Denials never expose private key material: certificate-derived strings
//!   are treated as identity evidence and are scrubbed of obvious secret
//!   markers via the existing [`CertificateIdentity`] guards.
//! - `forbid(unsafe_code)` is inherited from the crate root.
//!
//! ## Out of scope (deferred)
//!
//! - X.509 parsing, SAN/SPIFFE extraction, and real fingerprint computation.
//! - Time-based revocation lists and rotation choreography.
//! - Group / role expansion (V0 stores raw permission grants only).

use std::collections::BTreeMap;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::events::{
    AdminOperation, CertificateIdentity, Permission, SecurityAuditOutcome, SecurityAuditTrace,
    SurfaceScope, UserPrincipal,
};
use crate::trace_id::TraceId;

// ---------------------------------------------------------------------------
// SurfaceAction — the V0 dispatch verb set.
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// PrincipalBinding & PrincipalRegistry
// ---------------------------------------------------------------------------

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
        if let Some(existing) = self.bindings.get(&key) {
            if existing.principal.principal_id != binding.principal.principal_id {
                return Err(security_error(
                    "certificate fingerprint already bound to a different principal id; explicit rotation required",
                ));
            }
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

// ---------------------------------------------------------------------------
// AuthorizationOutcome
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// SurfaceAuthorizer
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::UserPrincipalKind;

    fn cert(fingerprint: &str, surface: SurfaceScope) -> CertificateIdentity {
        CertificateIdentity::new(fingerprint, format!("CN={}", fingerprint), surface).unwrap()
    }

    fn principal(id: &str) -> UserPrincipal {
        UserPrincipal::new(id, UserPrincipalKind::Service).unwrap()
    }

    fn binding(
        fingerprint: &str,
        surface: SurfaceScope,
        principal_id: &str,
        permissions: Vec<Permission>,
    ) -> PrincipalBinding {
        PrincipalBinding::new(
            cert(fingerprint, surface),
            principal(principal_id),
            permissions,
        )
        .unwrap()
    }

    fn registry_with(bindings: Vec<PrincipalBinding>) -> PrincipalRegistry {
        let mut registry = PrincipalRegistry::new();
        for b in bindings {
            registry.register(b).unwrap();
        }
        registry
    }

    #[test]
    fn binding_dedupes_repeated_permission_grants() {
        let b = binding(
            "fp-app-1",
            SurfaceScope::Application,
            "svc-1",
            vec![
                Permission::ExecuteProcedure,
                Permission::ExecuteProcedure,
                Permission::ReadContract,
            ],
        );
        assert_eq!(b.permissions().len(), 2);
    }

    #[test]
    fn binding_rejects_certificate_without_identity_evidence() {
        let bad = CertificateIdentity::new("fp", "subject", SurfaceScope::Application);
        // baseline: this construction is fine, then exercise the missing-evidence guard via empty fields
        assert!(bad.is_ok());

        let err = CertificateIdentity::new("", "subject", SurfaceScope::Application);
        assert!(
            err.is_err(),
            "empty fingerprint must be rejected at the source"
        );
    }

    #[test]
    fn registry_rejects_fingerprint_collision_with_different_principal() {
        let mut reg = PrincipalRegistry::new();
        reg.register(binding("fp-1", SurfaceScope::Application, "svc-a", vec![]))
            .unwrap();
        let err = reg
            .register(binding("fp-1", SurfaceScope::Application, "svc-b", vec![]))
            .unwrap_err();
        assert!(format!("{err}").contains("rotation"));
    }

    #[test]
    fn registry_allows_idempotent_re_register_for_same_principal() {
        let mut reg = PrincipalRegistry::new();
        reg.register(binding(
            "fp-1",
            SurfaceScope::Application,
            "svc-a",
            vec![Permission::ExecuteProcedure],
        ))
        .unwrap();
        reg.register(binding(
            "fp-1",
            SurfaceScope::Application,
            "svc-a",
            vec![Permission::ExecuteProcedure, Permission::ReadContract],
        ))
        .unwrap();
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.lookup("fp-1").unwrap().permissions().len(), 2);
    }

    #[test]
    fn unknown_certificate_denies_with_audit_evidence() {
        let reg = registry_with(vec![]);
        let auth = SurfaceAuthorizer::new(&reg);

        let outcome = auth
            .authorize(
                TraceId::new(1),
                SurfaceScope::Application,
                "fp-missing",
                SurfaceAction::ExecuteProcedure,
            )
            .unwrap();

        assert!(outcome.is_denied());
        let audit = outcome.audit();
        assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
        assert!(audit.has_reason());
        assert!(audit.has_supported_schema_version());
        assert!(audit.has_identity_evidence());
        if let AuthorizationOutcome::Denied { reason, .. } = outcome {
            assert_eq!(reason, AuthorizationDenialReason::UnknownCertificate);
        }
    }

    #[test]
    fn surface_scope_mismatch_denies_even_with_permission_grant() {
        let reg = registry_with(vec![binding(
            "fp-app-1",
            SurfaceScope::Application,
            "svc-1",
            vec![Permission::ExecuteProcedure],
        )]);
        let auth = SurfaceAuthorizer::new(&reg);

        let outcome = auth
            .authorize(
                TraceId::new(2),
                SurfaceScope::Administration,
                "fp-app-1",
                SurfaceAction::ExecuteProcedure,
            )
            .unwrap();

        assert!(outcome.is_denied());
        if let AuthorizationOutcome::Denied { reason, audit } = outcome {
            assert_eq!(reason, AuthorizationDenialReason::SurfaceScopeMismatch);
            assert!(!audit.surface_matches_certificate());
        }
    }

    #[test]
    fn surface_does_not_permit_admin_permission_on_application() {
        let reg = registry_with(vec![binding(
            "fp-app-1",
            SurfaceScope::Application,
            "svc-1",
            vec![Permission::ManageSecurity],
        )]);
        let auth = SurfaceAuthorizer::new(&reg);

        let outcome = auth
            .authorize(
                TraceId::new(3),
                SurfaceScope::Application,
                "fp-app-1",
                SurfaceAction::Admin(AdminOperation::ManageSecurity),
            )
            .unwrap();

        assert!(outcome.is_denied());
        if let AuthorizationOutcome::Denied { reason, .. } = outcome {
            assert_eq!(
                reason,
                AuthorizationDenialReason::SurfaceDoesNotPermitPermission
            );
        }
    }

    #[test]
    fn principal_missing_permission_denies_with_typed_reason() {
        let reg = registry_with(vec![binding(
            "fp-adm-1",
            SurfaceScope::Administration,
            "ops-1",
            vec![Permission::Backup],
        )]);
        let auth = SurfaceAuthorizer::new(&reg);

        let outcome = auth
            .authorize(
                TraceId::new(4),
                SurfaceScope::Administration,
                "fp-adm-1",
                SurfaceAction::Admin(AdminOperation::ManageSecurity),
            )
            .unwrap();

        assert!(outcome.is_denied());
        if let AuthorizationOutcome::Denied { reason, audit } = outcome {
            assert_eq!(
                reason,
                AuthorizationDenialReason::PrincipalMissingPermission
            );
            assert!(audit.permission == Permission::ManageSecurity);
        }
    }

    #[test]
    fn happy_path_allows_and_emits_audit_for_dispatch() {
        let reg = registry_with(vec![binding(
            "fp-app-1",
            SurfaceScope::Application,
            "svc-1",
            vec![Permission::ExecuteProcedure, Permission::ReadContract],
        )]);
        let auth = SurfaceAuthorizer::new(&reg);

        let outcome = auth
            .authorize(
                TraceId::new(5),
                SurfaceScope::Application,
                "fp-app-1",
                SurfaceAction::ExecuteProcedure,
            )
            .unwrap();

        assert!(outcome.is_allowed());
        let audit = outcome.audit();
        assert_eq!(audit.outcome, SecurityAuditOutcome::Allowed);
        assert!(audit.has_reason());
        assert!(audit.has_supported_schema_version());
        assert!(audit.has_identity_evidence());
        assert!(audit.surface_matches_certificate());
        assert!(audit.surface_permits_permission());
        assert!(!audit.contains_sensitive_evidence());

        if let AuthorizationOutcome::Allowed {
            principal,
            permission,
            ..
        } = outcome
        {
            assert_eq!(principal.principal_id, "svc-1");
            assert_eq!(permission, Permission::ExecuteProcedure);
        }
    }

    #[test]
    fn admin_action_required_permission_matches_admin_op() {
        for op in [
            AdminOperation::DebugProcedure,
            AdminOperation::ManageSecurity,
            AdminOperation::Backup,
            AdminOperation::ClusterPromote,
        ] {
            let action = SurfaceAction::Admin(op);
            assert_eq!(action.required_permission(), op.required_permission());
            assert!(action.is_admin());
        }
    }

    #[test]
    fn fingerprint_evidence_is_sanitized_for_unknown_certs() {
        let reg = registry_with(vec![]);
        let auth = SurfaceAuthorizer::new(&reg);

        let outcome = auth
            .authorize(
                TraceId::new(6),
                SurfaceScope::Application,
                "   ",
                SurfaceAction::ExecuteProcedure,
            )
            .unwrap();
        assert!(outcome.is_denied());
        let audit = outcome.audit();
        assert!(audit.certificate.fingerprint.starts_with("fingerprint:"));
        assert!(!audit.contains_sensitive_evidence());
    }
}
