use super::{
    error::{BackupResult, backup_error},
    plan::{BackupManifest, validate_wal_segment_chain},
    primitives::{BackupCatalogVersion, BackupLsn, BackupTraceId, WAL_FORMAT_VERSION},
    types::{BACKUP_PHYSICAL_PLAN_VERSION_V0, BACKUP_SUPPORTED_STORAGE_FORMAT_VERSION_V0},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupArtifactDigest {
    pub sha256: [u8; 32],
    pub crc64: u64,
    pub byte_len: u64,
}

impl BackupArtifactDigest {
    pub fn validate(&self, label: &str) -> BackupResult<()> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupCompatibility<C = super::CatalogVersion> {
    pub plan_version: u16,
    pub catalog_version: C,
    pub storage_format_version: u16,
    pub wal_format_version: u16,
}

impl<C: BackupCatalogVersion> BackupCompatibility<C> {
    pub fn validate(&self) -> BackupResult<()> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupArtifactCompatibilityEvidence {
    pub manifest_format_version: u16,
    pub physical_plan_version: u16,
    pub storage_format_version: u16,
    pub wal_format_version: u16,
    pub recorded_in_manifest: bool,
}

impl BackupArtifactCompatibilityEvidence {
    pub const fn recorded(manifest_format_version: u16) -> Self {
        Self {
            manifest_format_version,
            physical_plan_version: BACKUP_PHYSICAL_PLAN_VERSION_V0,
            storage_format_version: BACKUP_SUPPORTED_STORAGE_FORMAT_VERSION_V0,
            wal_format_version: WAL_FORMAT_VERSION,
            recorded_in_manifest: true,
        }
    }

    pub const fn reconstructed_legacy(manifest_format_version: u16) -> Self {
        Self {
            manifest_format_version,
            physical_plan_version: BACKUP_PHYSICAL_PLAN_VERSION_V0,
            storage_format_version: BACKUP_SUPPORTED_STORAGE_FORMAT_VERSION_V0,
            wal_format_version: WAL_FORMAT_VERSION,
            recorded_in_manifest: false,
        }
    }

    pub fn validate(&self) -> BackupResult<()> {
        if self.manifest_format_version == 0 {
            return Err(backup_error(
                "backup artifact compatibility manifest format version must not be zero",
            ));
        }
        if self.physical_plan_version != BACKUP_PHYSICAL_PLAN_VERSION_V0 {
            return Err(backup_error(
                "unsupported backup artifact physical plan version",
            ));
        }
        if self.storage_format_version != BACKUP_SUPPORTED_STORAGE_FORMAT_VERSION_V0 {
            return Err(backup_error(
                "unsupported backup artifact storage format version",
            ));
        }
        if self.wal_format_version != WAL_FORMAT_VERSION {
            return Err(backup_error(
                "unsupported backup artifact WAL format version",
            ));
        }
        Ok(())
    }
}

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
    pub fn validate_against<L: BackupLsn>(&self, manifest: &BackupManifest<L>) -> BackupResult<()> {
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
        if self.manifest_version != manifest.created_epoch {
            return Err(backup_error(
                "cold snapshot artifact manifest version must match backup manifest epoch",
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
        if self.manifest_crc != manifest.manifest_crc {
            return Err(backup_error(
                "cold snapshot artifact manifest CRC must match backup manifest",
            ));
        }
        self.artifact.validate("cold snapshot artifact")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupWalSegmentArtifact<L = super::Lsn> {
    pub segment_id: u64,
    pub first_lsn: L,
    pub last_lsn: L,
    pub base_previous_lsn: Option<L>,
    pub record_count: u64,
    pub wal_format_version: u16,
    pub artifact: BackupArtifactDigest,
}

impl<L: BackupLsn> BackupWalSegmentArtifact<L> {
    pub fn validate(&self) -> BackupResult<()> {
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
            Some(previous) if previous.try_next()? != self.first_lsn => {
                return Err(backup_error(
                    "backup WAL segment base previous LSN must chain to first LSN",
                ));
            }
            Some(_) | None => {}
        }
        self.artifact.validate("backup WAL segment artifact")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupPhysicalArtifactSet<L = super::Lsn> {
    pub backup_manifest: BackupArtifactDigest,
    pub cold_snapshot: BackupColdSnapshotArtifact,
    pub wal_segments: Vec<BackupWalSegmentArtifact<L>>,
}

impl<L: BackupLsn> BackupPhysicalArtifactSet<L> {
    pub fn validate_against(&self, manifest: &BackupManifest<L>) -> BackupResult<()> {
        self.backup_manifest.validate("backup manifest artifact")?;
        self.cold_snapshot.validate_against(manifest)?;
        validate_wal_segment_chain(manifest.wal_archive, &self.wal_segments)
    }

    fn wal_archive_bytes(&self) -> BackupResult<u64> {
        let mut total = 0_u64;
        for segment in &self.wal_segments {
            total = total
                .checked_add(segment.artifact.byte_len)
                .ok_or_else(|| backup_error("backup WAL archive byte length overflows u64"))?;
        }
        Ok(total)
    }
}

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
    pub fn validate_against<L: BackupLsn>(
        &self,
        artifacts: &BackupPhysicalArtifactSet<L>,
    ) -> BackupResult<()> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupIncompleteTransactionBoundary<L = super::Lsn> {
    pub durable_wal_end_lsn: L,
    pub highest_committed_lsn: L,
    pub incomplete_transaction_count: u64,
    pub policy: BackupIncompleteTransactionPolicy,
}

impl<L: BackupLsn> BackupIncompleteTransactionBoundary<L> {
    pub fn validate_against(&self, manifest: &BackupManifest<L>) -> BackupResult<()> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupAuditTraceFields<T = super::TraceId> {
    pub trace_id: T,
    pub requested_epoch: u64,
    pub completed_epoch: u64,
    pub actor_id_hash: [u8; 32],
    pub decision_reason_hash: [u8; 32],
}

impl<T: BackupTraceId> BackupAuditTraceFields<T> {
    pub fn validate(&self) -> BackupResult<()> {
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
