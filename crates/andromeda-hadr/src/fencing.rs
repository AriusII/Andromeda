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

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::types::{HadrEpoch, HadrNodeId};

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
            Self::PrimaryIdMismatch => {
                "hadr fencing token primary id does not match active primary"
            }
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
        return Err(HadrFencingRejection::StaleEpoch.into_error());
    }
    Ok(active)
}
