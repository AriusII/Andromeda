//! Cluster-scope security evidence for HADR control-plane decisions.
//!
//! HADR promotion, fencing, and manifest updates are cluster operations. This
//! module keeps the storage-side runtime checks explicit without importing IAM
//! policy mutation code into storage.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::{
    AdminOperation, Permission, SecurityAuditOutcome, SecurityAuditTrace, SurfaceScope,
};

use super::types::HadrEpoch;

const HADR_CLUSTER_MANIFEST_REASON_MAX_BYTES: usize = 512;

/// HADR cluster operation guarded by cluster-scope security audit evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HadrClusterOperation {
    PromotePrimary,
    FenceNode,
    UpdateManifest,
}

impl HadrClusterOperation {
    pub const fn admin_operation(self) -> AdminOperation {
        match self {
            Self::PromotePrimary => AdminOperation::ClusterPromote,
            Self::FenceNode => AdminOperation::FenceNode,
            Self::UpdateManifest => AdminOperation::UpdateClusterManifest,
        }
    }

    pub const fn required_permission(self) -> Permission {
        match self {
            Self::PromotePrimary => Permission::ClusterPromote,
            Self::FenceNode => Permission::FenceNode,
            Self::UpdateManifest => Permission::UpdateClusterManifest,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::PromotePrimary => "cluster primary promotion",
            Self::FenceNode => "cluster node fencing",
            Self::UpdateManifest => "cluster manifest update",
        }
    }
}

/// Validated cluster-scope authorization and durable security audit evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrClusterSecurityEvidence {
    operation: HadrClusterOperation,
    audit: SecurityAuditTrace,
}

impl HadrClusterSecurityEvidence {
    pub fn new(
        operation: HadrClusterOperation,
        audit: SecurityAuditTrace,
    ) -> AndromedaResult<Self> {
        validate_cluster_security_audit(operation, &audit)?;
        Ok(Self { operation, audit })
    }

    pub const fn operation(&self) -> HadrClusterOperation {
        self.operation
    }

    pub const fn audit(&self) -> &SecurityAuditTrace {
        &self.audit
    }

    pub fn require_operation(&self, operation: HadrClusterOperation) -> AndromedaResult<()> {
        if self.operation != operation {
            return Err(hadr_security_error(format!(
                "HADR {} requires matching cluster security evidence, got {} evidence",
                operation.label(),
                self.operation.label()
            )));
        }
        Ok(())
    }
}

fn validate_cluster_security_audit(
    operation: HadrClusterOperation,
    audit: &SecurityAuditTrace,
) -> AndromedaResult<()> {
    if audit.trace_id.is_zero() {
        return Err(hadr_security_error(
            "HADR cluster security audit evidence requires a non-zero trace id",
        ));
    }
    if audit.surface != SurfaceScope::Cluster {
        return Err(hadr_security_error(
            "HADR cluster security audit evidence must use Cluster surface",
        ));
    }
    if !audit.surface_matches_certificate() {
        return Err(hadr_security_error(
            "HADR cluster security audit certificate surface must match trace surface",
        ));
    }
    if !audit.surface_permits_permission() {
        return Err(hadr_security_error(
            "HADR cluster security audit surface must permit the requested permission",
        ));
    }
    if audit.permission != operation.required_permission()
        || !audit
            .permission
            .authorizes_admin_operation(operation.admin_operation())
    {
        return Err(hadr_security_error(format!(
            "HADR {} requires {:?} permission",
            operation.label(),
            operation.required_permission()
        )));
    }
    if audit.outcome != SecurityAuditOutcome::Allowed {
        return Err(hadr_security_error(
            "HADR cluster security audit evidence must be an allowed decision",
        ));
    }
    if !audit.has_policy_version_evidence() {
        return Err(hadr_security_error(
            "HADR cluster security audit evidence requires policy version evidence",
        ));
    }
    if !audit.has_identity_evidence() {
        return Err(hadr_security_error(
            "HADR cluster security audit evidence requires certificate and principal evidence",
        ));
    }
    if !audit.has_reason() {
        return Err(hadr_security_error(
            "HADR cluster security audit evidence requires a reason",
        ));
    }
    if audit.contains_sensitive_evidence() {
        return Err(hadr_security_error(
            "HADR cluster security audit evidence must not contain sensitive markers",
        ));
    }
    Ok(())
}

/// Monotonic durable cluster manifest version used by HADR control-plane state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HadrClusterManifestVersion(u64);

impl HadrClusterManifestVersion {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Request to update the HADR cluster manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrClusterManifestUpdateRequest {
    pub previous_epoch: HadrEpoch,
    pub proposed_epoch: HadrEpoch,
    pub previous_manifest_version: HadrClusterManifestVersion,
    pub proposed_manifest_version: HadrClusterManifestVersion,
    pub quorum_size: usize,
    pub granted_votes: usize,
    pub reason: String,
}

impl HadrClusterManifestUpdateRequest {
    #[allow(
        clippy::too_many_arguments,
        reason = "Manifest update evidence keeps epoch, version, quorum, and reason explicit."
    )]
    pub fn new(
        previous_epoch: HadrEpoch,
        proposed_epoch: HadrEpoch,
        previous_manifest_version: HadrClusterManifestVersion,
        proposed_manifest_version: HadrClusterManifestVersion,
        quorum_size: usize,
        granted_votes: usize,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            previous_epoch,
            proposed_epoch,
            previous_manifest_version,
            proposed_manifest_version,
            quorum_size,
            granted_votes,
            reason: reason.into(),
        }
    }
}

/// Validated evidence allowing a HADR manifest update to become visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrClusterManifestUpdateEvidence {
    pub request: HadrClusterManifestUpdateRequest,
    pub security: HadrClusterSecurityEvidence,
}

impl HadrClusterManifestUpdateEvidence {
    pub fn new(
        request: HadrClusterManifestUpdateRequest,
        security: HadrClusterSecurityEvidence,
    ) -> AndromedaResult<Self> {
        security.require_operation(HadrClusterOperation::UpdateManifest)?;
        validate_manifest_update_request(&request)?;
        Ok(Self { request, security })
    }
}

fn validate_manifest_update_request(
    request: &HadrClusterManifestUpdateRequest,
) -> AndromedaResult<()> {
    if request.proposed_epoch <= request.previous_epoch {
        return Err(hadr_security_error(
            "HADR manifest update proposed epoch must advance previous epoch",
        ));
    }
    if request.proposed_manifest_version <= request.previous_manifest_version {
        return Err(hadr_security_error(
            "HADR manifest update version must advance previous manifest version",
        ));
    }
    if request.quorum_size == 0 {
        return Err(hadr_security_error(
            "HADR manifest update quorum size must be non-zero",
        ));
    }
    if request.granted_votes < request.quorum_size {
        return Err(hadr_security_error(
            "HADR manifest update requires quorum-granted evidence",
        ));
    }
    if request.reason.trim().is_empty() {
        return Err(hadr_security_error(
            "HADR manifest update evidence requires a reason",
        ));
    }
    if request.reason.len() > HADR_CLUSTER_MANIFEST_REASON_MAX_BYTES {
        return Err(hadr_security_error(
            "HADR manifest update reason exceeds bounded evidence length",
        ));
    }
    Ok(())
}

fn hadr_security_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
