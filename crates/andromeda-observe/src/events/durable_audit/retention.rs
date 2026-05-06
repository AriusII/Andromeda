use andromeda_core::AndromedaResult;

use crate::events::{contains_sensitive_marker, observe_error};

use super::{DurableAuditRecordIdentity, DurableAuditReplayRecord, DurableAuditWalEvidence};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditRetentionBoundary {
    WalSegment,
    CatalogVersion,
    SecurityPolicy,
    ForensicHold,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditRetentionPolicy {
    pub min_record_lsn: Option<u64>,
    pub min_durable_lsn: Option<u64>,
    pub preserve_forensic_hold: bool,
    pub retain_boundaries: Vec<DurableAuditRetentionBoundary>,
}

impl DurableAuditRetentionPolicy {
    pub fn retain_all() -> Self {
        Self::default()
    }

    pub fn retain_record_lsn_at_or_after(min_record_lsn: u64) -> Self {
        Self {
            min_record_lsn: Some(min_record_lsn),
            ..Self::default()
        }
    }

    pub fn retain_durable_lsn_at_or_after(min_durable_lsn: u64) -> Self {
        Self {
            min_durable_lsn: Some(min_durable_lsn),
            ..Self::default()
        }
    }

    pub fn with_forensic_hold_preserved(mut self, preserve_forensic_hold: bool) -> Self {
        self.preserve_forensic_hold = preserve_forensic_hold;
        self
    }

    pub fn with_retention_boundary(mut self, boundary: DurableAuditRetentionBoundary) -> Self {
        if !self.retain_boundaries.contains(&boundary) {
            self.retain_boundaries.push(boundary);
        }
        self
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.min_record_lsn == Some(0) {
            return Err(observe_error(
                "durable audit retention policy min_record_lsn must be non-zero when present",
            ));
        }
        if self.min_durable_lsn == Some(0) {
            return Err(observe_error(
                "durable audit retention policy min_durable_lsn must be non-zero when present",
            ));
        }
        Ok(())
    }

    pub fn retains(&self, record: &DurableAuditReplayRecord) -> bool {
        if self.has_no_expiry_criteria() {
            return true;
        }
        if self.preserve_forensic_hold
            && record.report.retention == DurableAuditRetentionBoundary::ForensicHold
        {
            return true;
        }
        if self.retain_boundaries.contains(&record.report.retention) {
            return true;
        }
        if self
            .min_record_lsn
            .is_some_and(|min_lsn| record.report.evidence.record_lsn >= min_lsn)
        {
            return true;
        }
        self.min_durable_lsn
            .is_some_and(|min_lsn| record.report.evidence.durable_lsn >= min_lsn)
    }

    fn has_no_expiry_criteria(&self) -> bool {
        self.min_record_lsn.is_none()
            && self.min_durable_lsn.is_none()
            && self.retain_boundaries.is_empty()
    }
}

impl Default for DurableAuditRetentionPolicy {
    fn default() -> Self {
        Self {
            min_record_lsn: None,
            min_durable_lsn: None,
            preserve_forensic_hold: true,
            retain_boundaries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditCompactionReport {
    pub records_scanned: usize,
    pub records_retained: usize,
    pub records_expired: usize,
    pub first_retained_lsn: Option<u64>,
    pub last_retained_lsn: Option<u64>,
    pub retained_checksum_evidence: u64,
}

impl DurableAuditCompactionReport {
    pub fn from_records(
        records_scanned: usize,
        retained: &[DurableAuditReplayRecord],
    ) -> AndromedaResult<Self> {
        let records_retained = retained.len();
        if records_retained > records_scanned {
            return Err(observe_error(
                "durable audit compaction cannot retain more records than it scanned",
            ));
        }

        let mut retained_checksum_evidence = 0u64;
        let mut previous_lsn = 0u64;
        for record in retained {
            record.validate()?;
            let lsn = record.report.evidence.record_lsn;
            if lsn <= previous_lsn {
                return Err(observe_error(
                    "durable audit compaction retained records must remain LSN ordered",
                ));
            }
            previous_lsn = lsn;
            retained_checksum_evidence = retained_checksum_evidence
                .wrapping_mul(1_099_511_628_211)
                .wrapping_add(record.report.evidence.checksum)
                .wrapping_add(lsn);
        }

        Ok(Self {
            records_scanned,
            records_retained,
            records_expired: records_scanned.saturating_sub(records_retained),
            first_retained_lsn: retained
                .first()
                .map(|record| record.report.evidence.record_lsn),
            last_retained_lsn: retained
                .last()
                .map(|record| record.report.evidence.record_lsn),
            retained_checksum_evidence,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditWalSegmentArchiveProof {
    pub archive_id: String,
    pub first_lsn: u64,
    pub last_lsn: u64,
    pub checksum: u64,
}

impl DurableAuditWalSegmentArchiveProof {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.archive_id.trim().is_empty() {
            return Err(observe_error(
                "durable audit WAL archive proof requires non-empty archive_id",
            ));
        }
        if contains_sensitive_marker(&self.archive_id) {
            return Err(observe_error(
                "durable audit WAL archive proof must not contain secret evidence",
            ));
        }
        if self.first_lsn == 0 || self.last_lsn == 0 || self.first_lsn > self.last_lsn {
            return Err(observe_error(
                "durable audit WAL archive proof requires a non-zero ordered LSN range",
            ));
        }
        if self.checksum == 0 {
            return Err(observe_error(
                "durable audit WAL archive proof requires non-zero checksum evidence",
            ));
        }
        Ok(())
    }

    pub const fn covers(&self, evidence: DurableAuditWalEvidence) -> bool {
        self.first_lsn <= evidence.record_lsn && evidence.durable_lsn <= self.last_lsn
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurableAuditPruneBlockReason {
    PolicyRetainsRecord,
    ForensicHold,
    SecurityPolicy,
    CatalogVersionRetentionBoundary,
    MissingWalSegmentArchiveProof,
    WalSegmentArchiveProofMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditPruneEvidence {
    pub identity: DurableAuditRecordIdentity,
    pub record_lsn: u64,
    pub durable_lsn: u64,
    pub record_checksum: u64,
    pub retention: DurableAuditRetentionBoundary,
    pub archive_proof: Option<DurableAuditWalSegmentArchiveProof>,
    pub prune_allowed: bool,
    pub blocked_by: Option<DurableAuditPruneBlockReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditRetentionManager {
    pub policy: DurableAuditRetentionPolicy,
}

impl DurableAuditRetentionManager {
    pub fn new(policy: DurableAuditRetentionPolicy) -> AndromedaResult<Self> {
        policy.validate()?;
        Ok(Self { policy })
    }

    pub fn evaluate_prune(
        &self,
        record: &DurableAuditReplayRecord,
        archive_proof: Option<DurableAuditWalSegmentArchiveProof>,
    ) -> AndromedaResult<DurableAuditPruneEvidence> {
        record.validate()?;
        self.policy.validate()?;
        if let Some(proof) = &archive_proof {
            proof.validate()?;
        }

        let block = match record.report.retention {
            DurableAuditRetentionBoundary::ForensicHold => {
                Some(DurableAuditPruneBlockReason::ForensicHold)
            }
            DurableAuditRetentionBoundary::SecurityPolicy => {
                Some(DurableAuditPruneBlockReason::SecurityPolicy)
            }
            _ if self.policy.retains(record) => {
                Some(DurableAuditPruneBlockReason::PolicyRetainsRecord)
            }
            DurableAuditRetentionBoundary::WalSegment => match &archive_proof {
                Some(proof)
                    if proof.covers(record.report.evidence)
                        && proof.checksum == record.report.evidence.checksum =>
                {
                    None
                }
                Some(_) => Some(DurableAuditPruneBlockReason::WalSegmentArchiveProofMismatch),
                None => Some(DurableAuditPruneBlockReason::MissingWalSegmentArchiveProof),
            },
            DurableAuditRetentionBoundary::CatalogVersion => {
                Some(DurableAuditPruneBlockReason::CatalogVersionRetentionBoundary)
            }
        };

        Ok(DurableAuditPruneEvidence {
            identity: record.report.identity,
            record_lsn: record.report.evidence.record_lsn,
            durable_lsn: record.report.evidence.durable_lsn,
            record_checksum: record.report.evidence.checksum,
            retention: record.report.retention,
            archive_proof,
            prune_allowed: block.is_none(),
            blocked_by: block,
        })
    }
}
