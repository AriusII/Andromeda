use andromeda_audit::{
    AdminOperation, CertificateIdentity, Permission, SecurityAuditOutcome, SecurityAuditTrace,
    SurfaceScope, UserPrincipal,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observability::TraceId;

use super::AuthorizationDenialReason;

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
    pub(super) certificate: CertificateIdentity,
    pub(super) principal: UserPrincipal,
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

/// Result of [`super::SurfaceAuthorizer::authorize`].
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
    /// Returns the audit trace regardless of allow/deny; every security
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

pub(crate) fn allowed_security_outcome(
    trace_id: TraceId,
    requested_scope: SurfaceScope,
    certificate: CertificateIdentity,
    principal: UserPrincipal,
    permission: Permission,
    reason_evidence: impl Into<String>,
) -> AndromedaResult<AuthorizationOutcome> {
    let audit = SecurityAuditTrace::new(
        trace_id,
        requested_scope,
        certificate,
        principal.clone(),
        permission,
        SecurityAuditOutcome::Allowed,
        format!("allowed:{}", reason_evidence.into()),
    )?;

    Ok(AuthorizationOutcome::Allowed {
        principal,
        permission,
        audit,
    })
}

pub(crate) fn denied_security_outcome(
    trace_id: TraceId,
    requested_scope: SurfaceScope,
    certificate: CertificateIdentity,
    principal: UserPrincipal,
    permission: Permission,
    reason: AuthorizationDenialReason,
    reason_evidence: impl Into<String>,
) -> AndromedaResult<AuthorizationOutcome> {
    let audit = SecurityAuditTrace::new(
        trace_id,
        requested_scope,
        certificate,
        principal,
        permission,
        SecurityAuditOutcome::Denied,
        format!("denied:{}:{}", reason.label(), reason_evidence.into()),
    )?;

    Ok(AuthorizationOutcome::Denied { reason, audit })
}

fn security_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Internal, message)
}
