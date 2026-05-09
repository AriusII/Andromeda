use andromeda_audit::{
    CertificateIdentity, SecurityAuditOutcome, SecurityAuditTrace, SurfaceScope, UserPrincipal,
    UserPrincipalKind,
};
use andromeda_error::AndromedaResult;
use andromeda_observability::TraceId;

use super::{AuthorizationDenialReason, AuthorizationOutcome, PrincipalRegistry, SurfaceAction};

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
    /// `requested_scope` is the surface the inbound transport asserts,
    /// `presented_fingerprint` is the fingerprint observed during the mTLS
    /// handshake, and `action` is the dispatch verb the request resolves to.
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
            let synth_principal =
                UserPrincipal::new("principal:unknown", UserPrincipalKind::Service)?;
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
        // the surface itself must structurally allow the permission family.
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

/// Sanitize the presented fingerprint for inclusion in an audit envelope.
fn fingerprint_for_audit(presented: &str) -> String {
    let trimmed = presented.trim();
    if trimmed.is_empty() {
        return "fingerprint:empty".to_string();
    }
    let truncated: String = trimmed.chars().take(64).collect();
    format!("fingerprint:{}", truncated)
}
