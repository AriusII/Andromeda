//! Durable artifact structures for physical backup execution.
//!
//! All fields here reference byte-level evidence from durable storage only —
//! no RAM page cache state, no GPU or pipeline-execution fields.

use andromeda_core::{AndromedaResult, CatalogVersion};
use andromeda_observe::TraceId;

use crate::{Lsn, WAL_FORMAT_VERSION};

use super::helpers::backup_error;
use super::plan::{validate_wal_segment_chain, BackupManifest};
use super::types::{BACKUP_PHYSICAL_PLAN_VERSION_V0, BACKUP_SUPPORTED_STORAGE_FORMAT_VERSION_V0};

/// Digest/checksum identity for a durable physical backup artifact.
///
/// This deliberately contains only durable byte evidence: no in-memory WAL
/// handles, no EventSink-derived truth, no GPU/runtime pipeline fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupArtifactDigest {
    pub sha256: [u8; 32],
    pub crc64: u64,
    pub byte_len: u64,
}

impl BackupArtifactDigest {
    pub fn validate(&self, label: &str) -> AndromedaResult<()> {
        if self.sha256 == [0; 32] {
            return Err(backup_error(format!(
                "{label} SHA-256 digest must not be zero"
            )));
        }
        if self.crc64 == 0 {
            return Err(backup_error(format!("{label} CRC64 must not be zero")));
        }
        if self.byte_len == 0 {
            return Err(backup_error(format!(
                "{label} byte length must not be zero"
            )));
        }
        Ok(())
    }
}

/// Compatibility tuple recorded with every physical backup plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupCompatibility {
    pub plan_version: u16,
    pub catalog_version: CatalogVersion,
    pub storage_format_version: u16,
    pub wal_format_version: u16,
}

impl BackupCompatibility {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.plan_version != BACKUP_PHYSICAL_PLAN_VERSION_V0 {
            return Err(backup_error("unsupported backup physical plan version"));
        }
        if self.catalog_version.get() == 0 {
            return Err(backup_error("backup catalog version must not be zero"));
        }
        if self.storage_format_version != BACKUP_SUPPORTED_STORAGE_FORMAT_VERSION_V0 {
            return Err(backup_error("unsupported backup storage format version"));
        }
        if self.wal_format_version != WAL_FORMAT_VERSION {
            return Err(backup_error("unsupported backup WAL format version"));
        }
        Ok(())
    }
}

/// Durable cold-snapshot artifact bound to the backup manifest identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupColdSnapshotArtifact {
    pub database_id: u64,
    pub manifest_version: u64,
    pub snapshot_id: u64,
    pub snapshot_descriptor_hash: [u8; 32],
    pub manifest_crc: u32,
    pub artifact: BackupArtifactDigest,
}

impl BackupColdSnapshotArtifact {
    pub fn validate_against(&self, manifest: &BackupManifest) -> AndromedaResult<()> {
        if self.database_id != manifest.database_id {
            return Err(backup_error(
                "cold snapshot artifact database id must match backup manifest",
            ));
        }
        if self.manifest_version == 0 {
            return Err(backup_error(
                "cold snapshot artifact manifest version must not be zero",
            ));
        }
        if self.snapshot_id != manifest.snapshot.snapshot_id {
            return Err(backup_error(
                "cold snapshot artifact id must match backup manifest snapshot",
            ));
        }
        if self.snapshot_descriptor_hash != manifest.snapshot.snapshot_descriptor_hash {
            return Err(backup_error(
                "cold snapshot artifact descriptor hash must match backup manifest",
            ));
        }
        if self.manifest_crc == 0 {
            return Err(backup_error(
                "cold snapshot artifact manifest CRC must not be zero",
            ));
        }
        self.artifact.validate("cold snapshot artifact")
    }
}

/// Durable WAL segment artifact participating in the backup WAL archive range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupWalSegmentArtifact {
    pub segment_id: u64,
    pub first_lsn: Lsn,
    pub last_lsn: Lsn,
    pub base_previous_lsn: Option<Lsn>,
    pub record_count: u64,
    pub wal_format_version: u16,
    pub artifact: BackupArtifactDigest,
}

impl BackupWalSegmentArtifact {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.segment_id == 0 {
            return Err(backup_error("backup WAL segment id must not be zero"));
        }
        if self.wal_format_version != WAL_FORMAT_VERSION {
            return Err(backup_error("backup WAL segment format version mismatch"));
        }
        if self.record_count == 0 {
            return Err(backup_error(
                "backup WAL segment record count must not be zero",
            ));
        }
        if self.first_lsn.is_zero() || self.last_lsn.is_zero() {
            return Err(backup_error(
                "backup WAL segment LSN bounds must not be zero",
            ));
        }
        if self.last_lsn < self.first_lsn {
            return Err(backup_error(
                "backup WAL segment last LSN precedes first LSN",
            ));
        }
        match self.base_previous_lsn {
            Some(previous) => {
                if previous.try_next()? != self.first_lsn {
                    return Err(backup_error(
                        "backup WAL segment base previous LSN must chain to first LSN",
                    ));
                }
            }
            None => {
                if self.first_lsn != Lsn::new(1) {
                    return Err(backup_error(
                        "backup WAL segment without base previous LSN must start at LSN 1",
                    ));
                }
            }
        }
        self.artifact.validate("backup WAL segment artifact")
    }
}

/// Exact durable artifact set expected from F5 physical backup execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupPhysicalArtifactSet {
    pub backup_manifest: BackupArtifactDigest,
    pub cold_snapshot: BackupColdSnapshotArtifact,
    pub wal_segments: Vec<BackupWalSegmentArtifact>,
}

impl BackupPhysicalArtifactSet {
    pub fn validate_against(&self, manifest: &BackupManifest) -> AndromedaResult<()> {
        self.backup_manifest.validate("backup manifest artifact")?;
        self.cold_snapshot.validate_against(manifest)?;
        validate_wal_segment_chain(manifest.wal_archive, &self.wal_segments)
    }

    pub(super) fn wal_archive_bytes(&self) -> AndromedaResult<u64> {
        let mut total = 0_u64;
        for segment in &self.wal_segments {
            total = total
                .checked_add(segment.artifact.byte_len)
                .ok_or_else(|| backup_error("backup WAL archive byte length overflows u64"))?;
        }
        Ok(total)
    }
}

/// Resource guardrails for a physical backup. Observed bytes are derived from
/// durable artifact sizes, never from RAM page cache state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupResourceBounds {
    pub max_snapshot_bytes: u64,
    pub max_wal_archive_bytes: u64,
    pub max_wal_segment_count: usize,
    pub observed_snapshot_bytes: u64,
    pub observed_wal_archive_bytes: u64,
    pub observed_wal_segment_count: usize,
}

impl BackupResourceBounds {
    pub fn validate_against(&self, artifacts: &BackupPhysicalArtifactSet) -> AndromedaResult<()> {
        if self.max_snapshot_bytes == 0
            || self.max_wal_archive_bytes == 0
            || self.max_wal_segment_count == 0
        {
            return Err(backup_error("backup resource maxima must not be zero"));
        }
        if self.observed_snapshot_bytes != artifacts.cold_snapshot.artifact.byte_len {
            return Err(backup_error(
                "backup observed snapshot bytes must match cold snapshot artifact",
            ));
        }
        let wal_bytes = artifacts.wal_archive_bytes()?;
        if self.observed_wal_archive_bytes != wal_bytes {
            return Err(backup_error(
                "backup observed WAL bytes must match WAL segment artifacts",
            ));
        }
        if self.observed_wal_segment_count != artifacts.wal_segments.len() {
            return Err(backup_error(
                "backup observed WAL segment count must match artifact list",
            ));
        }
        if self.observed_snapshot_bytes > self.max_snapshot_bytes {
            return Err(backup_error("backup snapshot bytes exceed resource bound"));
        }
        if self.observed_wal_archive_bytes > self.max_wal_archive_bytes {
            return Err(backup_error("backup WAL bytes exceed resource bound"));
        }
        if self.observed_wal_segment_count > self.max_wal_segment_count {
            return Err(backup_error(
                "backup WAL segment count exceeds resource bound",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupIncompleteTransactionPolicy {
    DiscardOnRecovery,
}

/// Durable boundary proof for incomplete transactions at the backup WAL end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupIncompleteTransactionBoundary {
    pub durable_wal_end_lsn: Lsn,
    pub highest_committed_lsn: Lsn,
    pub incomplete_transaction_count: u64,
    pub policy: BackupIncompleteTransactionPolicy,
}

impl BackupIncompleteTransactionBoundary {
    pub fn validate_against(&self, manifest: &BackupManifest) -> AndromedaResult<()> {
        if self.durable_wal_end_lsn != manifest.wal_archive.end_inclusive {
            return Err(backup_error(
                "backup incomplete transaction boundary must end at WAL archive end",
            ));
        }
        if self.highest_committed_lsn > self.durable_wal_end_lsn {
            return Err(backup_error(
                "backup highest committed LSN must not exceed durable WAL end",
            ));
        }
        if self.incomplete_transaction_count > 0
            && self.policy != BackupIncompleteTransactionPolicy::DiscardOnRecovery
        {
            return Err(backup_error(
                "backup incomplete transactions must be marked discard-on-recovery",
            ));
        }
        Ok(())
    }
}

/// Audit fields that must accompany a backup decision when execution wiring is
/// added. Identity values are hashes/ids only; no ad hoc SQL or runtime JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupAuditTraceFields {
    pub trace_id: TraceId,
    pub requested_epoch: u64,
    pub completed_epoch: u64,
    pub actor_id_hash: [u8; 32],
    pub decision_reason_hash: [u8; 32],
}

impl BackupAuditTraceFields {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.trace_id.is_zero() {
            return Err(backup_error("backup audit trace id must not be zero"));
        }
        if self.requested_epoch == 0 || self.completed_epoch == 0 {
            return Err(backup_error("backup audit epochs must not be zero"));
        }
        if self.completed_epoch < self.requested_epoch {
            return Err(backup_error(
                "backup audit completed epoch must not precede requested epoch",
            ));
        }
        if self.actor_id_hash == [0; 32] {
            return Err(backup_error("backup audit actor hash must not be zero"));
        }
        if self.decision_reason_hash == [0; 32] {
            return Err(backup_error(
                "backup audit decision reason hash must not be zero",
            ));
        }
        Ok(())
    }
}
