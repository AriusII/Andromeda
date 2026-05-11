use andromeda_error::AndromedaResult;

use crate::{
    Lsn,
    cluster_security::{HadrClusterDurableAuditProof, HadrClusterSecurityEvidence},
    fencing::HadrFencingToken,
    quorum::HadrAuditRecord,
    types::{HadrEpoch, HadrNodeId},
};

use super::promotion_error;

/// Durable audit marker that must be appended before a primary becomes visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrPromotionAuditMarker {
    pub candidate_id: HadrNodeId,
    pub proposed_epoch: HadrEpoch,
    pub primary_durable_lsn: Lsn,
    pub committed_safe_lsn: Lsn,
    pub token: HadrFencingToken,
    pub audit_record: HadrAuditRecord,
    pub cluster_security: Option<HadrClusterSecurityEvidence>,
}

/// Durable receipt returned by a promotion audit sink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HadrPromotionAuditReceipt {
    pub audit_lsn: Lsn,
    pub marker_digest_sha256: [u8; 32],
}

impl HadrPromotionAuditReceipt {
    pub fn new(audit_lsn: Lsn, marker_digest_sha256: [u8; 32]) -> AndromedaResult<Self> {
        if audit_lsn == Lsn::ZERO {
            return Err(promotion_error(
                "HADR promotion audit receipt requires a non-zero audit LSN",
            ));
        }
        if marker_digest_sha256.iter().all(|byte| *byte == 0) {
            return Err(promotion_error(
                "HADR promotion audit receipt requires a non-zero marker digest",
            ));
        }
        Ok(Self {
            audit_lsn,
            marker_digest_sha256,
        })
    }
}

impl TryFrom<HadrPromotionAuditReceipt> for HadrClusterDurableAuditProof {
    type Error = andromeda_error::AndromedaError;

    fn try_from(value: HadrPromotionAuditReceipt) -> Result<Self, Self::Error> {
        HadrClusterDurableAuditProof::new(value.audit_lsn, value.marker_digest_sha256)
    }
}

/// Audit sink used by the storage-side promotion boundary.
pub trait HadrPromotionAuditLog {
    fn append_primary_promotion_marker(
        &self,
        marker: &HadrPromotionAuditMarker,
    ) -> AndromedaResult<()>;

    fn append_primary_promotion_marker_durably(
        &self,
        marker: &HadrPromotionAuditMarker,
    ) -> AndromedaResult<HadrPromotionAuditReceipt> {
        let _ = marker;
        Err(promotion_error(
            "HADR promotion audit log did not return a durable audit receipt",
        ))
    }
}

/// No-op audit sink for callers that only need the membership-store records.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopPromotionAuditLog;

impl HadrPromotionAuditLog for NoopPromotionAuditLog {
    fn append_primary_promotion_marker(
        &self,
        _marker: &HadrPromotionAuditMarker,
    ) -> AndromedaResult<()> {
        Ok(())
    }
}
