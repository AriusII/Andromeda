use andromeda_audit::{CertificateIdentity, SurfaceScope, UserPrincipal, UserPrincipalKind};
use andromeda_error::AndromedaResult;
use andromeda_observability::TraceId;

use crate::BreakGlassPolicy;

use super::{
    AuthorizationDenialReason, AuthorizationOutcome, PrincipalRegistry, SurfaceAction,
    allowed_security_outcome, denied_security_outcome,
};

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
            return denied_security_outcome(
                trace_id,
                requested_scope,
                synth_cert,
                synth_principal,
                permission,
                AuthorizationDenialReason::UnknownCertificate,
                format!("action={}", action.evidence_label()),
            );
        };

        // Scope mismatch: the certificate was issued for one surface but
        // is being presented on another. This is a surface-isolation
        // violation regardless of permission grants.
        if binding.certificate.surface != requested_scope {
            return denied_security_outcome(
                trace_id,
                requested_scope,
                binding.certificate.clone(),
                binding.principal.clone(),
                permission,
                AuthorizationDenialReason::SurfaceScopeMismatch,
                format!(
                    "cert_surface={:?}:requested_surface={:?}:action={}",
                    binding.certificate.surface,
                    requested_scope,
                    action.evidence_label(),
                ),
            );
        }

        // Surface-scope policy: even if the principal has the permission,
        // the surface itself must structurally allow the permission family.
        if !requested_scope.permits_permission(permission) {
            return denied_security_outcome(
                trace_id,
                requested_scope,
                binding.certificate.clone(),
                binding.principal.clone(),
                permission,
                AuthorizationDenialReason::SurfaceDoesNotPermitPermission,
                format!(
                    "surface={:?}:permission={:?}:action={}",
                    requested_scope,
                    permission,
                    action.evidence_label(),
                ),
            );
        }

        // Principal grant check.
        if !binding.grants(permission) {
            return denied_security_outcome(
                trace_id,
                requested_scope,
                binding.certificate.clone(),
                binding.principal.clone(),
                permission,
                AuthorizationDenialReason::PrincipalMissingPermission,
                format!(
                    "permission={:?}:action={}",
                    permission,
                    action.evidence_label(),
                ),
            );
        }

        allowed_security_outcome(
            trace_id,
            requested_scope,
            binding.certificate.clone(),
            binding.principal.clone(),
            permission,
            format!(
                "permission={:?}:action={}",
                permission,
                action.evidence_label()
            ),
        )
    }

    /// Authorize and optionally apply a bounded break-glass override.
    ///
    /// Override is allowed only when:
    /// - baseline decision is denied
    /// - certificate resolves to a known break-glass principal
    /// - policy is active at `now_epoch_secs`
    /// - requested scope + permission are explicitly listed in the policy
    pub fn authorize_with_break_glass(
        &self,
        trace_id: TraceId,
        requested_scope: SurfaceScope,
        presented_fingerprint: &str,
        action: SurfaceAction,
        now_epoch_secs: u64,
        break_glass: Option<&BreakGlassPolicy>,
    ) -> AndromedaResult<AuthorizationOutcome> {
        let baseline = self.authorize(trace_id, requested_scope, presented_fingerprint, action)?;
        if baseline.is_allowed() {
            return Ok(baseline);
        }

        let Some(policy) = break_glass else {
            return Ok(baseline);
        };

        let Some(binding) = self.registry.lookup(presented_fingerprint) else {
            return Ok(baseline);
        };

        if binding.principal.kind != UserPrincipalKind::BreakGlass {
            return Ok(baseline);
        }

        let permission = action.required_permission();
        if !policy.allows(requested_scope, permission, now_epoch_secs) {
            return Ok(baseline);
        }

        allowed_security_outcome(
            trace_id,
            requested_scope,
            binding.certificate.clone(),
            binding.principal.clone(),
            permission,
            format!(
                "break_glass_override:ticket={}:approved_by={}:permission={:?}:action={}",
                policy.ticket_id(),
                policy.approved_by(),
                permission,
                action.evidence_label()
            ),
        )
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
