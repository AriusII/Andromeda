//! Fencing token issuance and enforcement for HADR primaries.
//!
//! A fencing token binds `(primary_id, epoch)`. Operations presented to the
//! cluster carry the token under which they were issued. Any token whose epoch
//! is below the cluster's currently active epoch is fenced off immediately.
//!
//! This module owns:
//! * [`HadrFencingToken`] — the authority credential issued on promotion.
//! * [`HadrFencingContext`] — the cluster-wide view of the active token and
//!   the highest epoch ever observed.
//! * [`HadrFencingRejection`] — categorical fence-off reasons.
//! * [`enforce_fencing_token`] — the pure validation function.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::cluster_security::{
    HadrClusterDurableAuditProof, HadrClusterSecurityEvidence, require_cluster_fence_admission,
};
use super::types::{HadrEpoch, HadrNodeId};

const HADR_SECURED_FENCING_REASON_MAX_BYTES: usize = 512;

/// Fencing token issued to a successfully promoted primary.
///
/// A token binds `(primary_id, epoch)`. Operations against the cluster carry
/// the token under which they were issued; any token whose epoch is below
/// the cluster's currently active epoch is fenced off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HadrFencingToken {
    pub primary_id: HadrNodeId,
    pub epoch: HadrEpoch,
}

impl HadrFencingToken {
    pub const fn new(primary_id: HadrNodeId, epoch: HadrEpoch) -> Self {
        Self { primary_id, epoch }
    }
}

/// Snapshot of cluster-wide fencing context at the moment a promotion is
/// being decided. `active_token` represents the currently authoritative
/// primary, if any. `highest_observed_epoch` represents the highest epoch
/// any node in the cluster has *ever* observed (across votes and prior
/// fencing tokens). The decision engine uses both to enforce strict epoch
/// monotonicity and to detect split-brain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HadrFencingContext {
    pub active_token: Option<HadrFencingToken>,
    pub highest_observed_epoch: HadrEpoch,
}

impl HadrFencingContext {
    pub const fn empty() -> Self {
        Self {
            active_token: None,
            highest_observed_epoch: HadrEpoch::ZERO,
        }
    }

    pub const fn with_active(token: HadrFencingToken, highest_observed_epoch: HadrEpoch) -> Self {
        Self {
            active_token: Some(token),
            highest_observed_epoch,
        }
    }
}

/// Categorical reason an operation was fenced off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HadrFencingRejection {
    /// The presented token's epoch is below the active epoch.
    StaleEpoch,
    /// The presented token's epoch is above the active epoch.
    FutureEpoch,
    /// The presented token's primary id does not match the active primary.
    PrimaryIdMismatch,
    /// The presented token's epoch matches the active epoch but the cluster
    /// has no active token at all (e.g. mid-failover).
    NoActiveToken,
}

impl HadrFencingRejection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StaleEpoch => "hadr fencing token epoch is below active epoch",
            Self::FutureEpoch => {
                "hadr fencing token epoch exceeds active epoch; possible split-brain evidence"
            },
            Self::PrimaryIdMismatch => {
                "hadr fencing token primary id does not match active primary"
            },
            Self::NoActiveToken => "hadr cluster has no active fencing token",
        }
    }

    pub(super) fn into_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Storage, self.as_str())
    }
}

/// Validate that an operation tagged with `presented` is permitted under
/// the current fencing context. Returns the active token on success.
pub fn enforce_fencing_token(
    presented: HadrFencingToken,
    fencing: &HadrFencingContext,
) -> AndromedaResult<HadrFencingToken> {
    let active = fencing
        .active_token
        .ok_or_else(|| HadrFencingRejection::NoActiveToken.into_error())?;
    if presented.epoch < active.epoch {
        return Err(HadrFencingRejection::StaleEpoch.into_error());
    }
    if presented.epoch == active.epoch && presented.primary_id != active.primary_id {
        return Err(HadrFencingRejection::PrimaryIdMismatch.into_error());
    }
    if presented.epoch > active.epoch {
        // A token strictly above the active epoch would itself be evidence
        // of split-brain (the cluster has not yet recorded that promotion).
        return Err(HadrFencingRejection::FutureEpoch.into_error());
    }
    Ok(active)
}

/// Typed request for a visible HADR fencing mutation.
///
/// The request carries the currently active fencing token plus quorum evidence
/// for the control-plane mutation. Security admission and durable audit receipt
/// proof are intentionally supplied separately to keep authorization evidence
/// typed and validated at the mutation boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrSecuredFencingRequest {
    pub presented_token: HadrFencingToken,
    pub quorum_size: usize,
    pub granted_votes: usize,
    pub reason: String,
}

impl HadrSecuredFencingRequest {
    pub fn new(
        presented_token: HadrFencingToken,
        quorum_size: usize,
        granted_votes: usize,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            presented_token,
            quorum_size,
            granted_votes,
            reason: reason.into(),
        }
    }
}

/// Validated evidence that a HADR fencing mutation may become visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrSecuredFencingEvidence {
    request: HadrSecuredFencingRequest,
    active_token: HadrFencingToken,
    security: HadrClusterSecurityEvidence,
    durable_audit: HadrClusterDurableAuditProof,
}

impl HadrSecuredFencingEvidence {
    pub fn new(
        request: HadrSecuredFencingRequest,
        fencing: &HadrFencingContext,
        security: HadrClusterSecurityEvidence,
        durable_audit: Option<&HadrClusterDurableAuditProof>,
    ) -> AndromedaResult<Self> {
        let durable_audit = match durable_audit {
            Some(proof) => *proof,
            None => {
                return Err(fencing_error(
                    "HADR cluster node fencing requires durable audit proof",
                ));
            },
        };
        require_cluster_fence_admission(&security, Some(&durable_audit))?;
        validate_secured_fencing_request(&request, fencing)?;
        let active_token = enforce_fencing_token(request.presented_token, fencing)?;
        Ok(Self {
            request,
            active_token,
            security,
            durable_audit,
        })
    }

    pub const fn request(&self) -> &HadrSecuredFencingRequest {
        &self.request
    }

    pub const fn active_token(&self) -> HadrFencingToken {
        self.active_token
    }

    pub const fn security(&self) -> &HadrClusterSecurityEvidence {
        &self.security
    }

    pub const fn durable_audit(&self) -> HadrClusterDurableAuditProof {
        self.durable_audit
    }
}

fn validate_secured_fencing_request(
    request: &HadrSecuredFencingRequest,
    fencing: &HadrFencingContext,
) -> AndromedaResult<()> {
    if request.presented_token.primary_id.is_zero() {
        return Err(fencing_error(
            "HADR secured fencing requires a non-zero primary id",
        ));
    }
    if request.presented_token.epoch.is_zero() {
        return Err(fencing_error(
            "HADR secured fencing requires a non-zero fencing epoch",
        ));
    }
    if request.presented_token.epoch < fencing.highest_observed_epoch {
        return Err(HadrFencingRejection::StaleEpoch.into_error());
    }
    if request.quorum_size == 0 {
        return Err(fencing_error(
            "HADR secured fencing quorum size must be non-zero",
        ));
    }
    if request.granted_votes < request.quorum_size {
        return Err(fencing_error(
            "HADR secured fencing requires quorum-granted evidence",
        ));
    }
    if request.reason.trim().is_empty() {
        return Err(fencing_error(
            "HADR secured fencing evidence requires a reason",
        ));
    }
    if request.reason.len() > HADR_SECURED_FENCING_REASON_MAX_BYTES {
        return Err(fencing_error(
            "HADR secured fencing reason exceeds bounded evidence length",
        ));
    }
    if contains_sensitive_marker(&request.reason) {
        return Err(fencing_error(
            "HADR secured fencing reason must not contain sensitive markers",
        ));
    }
    Ok(())
}

fn fencing_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

fn contains_sensitive_marker(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    [
        "-----begin",
        "private key",
        "private_key",
        "bearer ",
        "credential=",
        "password=",
        "passwd=",
        "secret=",
        "token=",
        "authorization:",
        "x-api-key",
        "payload:",
        "payload body",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}
