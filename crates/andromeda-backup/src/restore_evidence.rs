use super::{
    error::{BackupResult, backup_error},
    plan::BackupManifest,
    primitives::BackupLsn,
    types::BackupId,
};

/// Evidence produced by a successful restore drill for a backup.
///
/// `RestoreEvidence` is immutable after construction: all fields are private
/// and no setters are exposed. Callers obtain it via `new()` which enforces
/// all C5 nonzero fences at construction time.
///
/// This type is the critical C5 gate for P13 exit criterion line 71:
/// "No backup marks recoverable without restore evidence."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestoreEvidence {
    backup_id: BackupId,
    manifest_crc: u32,
    restore_drill_id: u64,
    drill_epoch: u64,
    target_lsn_verified: u64,
    drill_digest: [u8; 32],
}

impl RestoreEvidence {
    /// Construct a `RestoreEvidence` from a completed restore drill.
    ///
    /// Fails closed if any field is zero or if `drill_digest` is all-zeros.
    pub fn new(
        backup_id: BackupId,
        manifest_crc: u32,
        restore_drill_id: u64,
        drill_epoch: u64,
        target_lsn_verified: u64,
        drill_digest: [u8; 32],
    ) -> BackupResult<Self> {
        let evidence = Self {
            backup_id,
            manifest_crc,
            restore_drill_id,
            drill_epoch,
            target_lsn_verified,
            drill_digest,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    pub const fn backup_id(&self) -> BackupId {
        self.backup_id
    }

    pub const fn manifest_crc(&self) -> u32 {
        self.manifest_crc
    }

    pub fn restore_drill_id(&self) -> u64 {
        self.restore_drill_id
    }

    pub fn drill_epoch(&self) -> u64 {
        self.drill_epoch
    }

    pub fn target_lsn_verified(&self) -> u64 {
        self.target_lsn_verified
    }

    pub fn drill_digest(&self) -> &[u8; 32] {
        &self.drill_digest
    }

    /// Validate that all fields are nonzero. Called by `new()` on construction.
    pub fn validate(&self) -> BackupResult<()> {
        if self.backup_id.is_zero() {
            return Err(backup_error("restore evidence backup id must not be zero"));
        }
        if self.manifest_crc == 0 {
            return Err(backup_error(
                "restore evidence manifest CRC must not be zero",
            ));
        }
        if self.restore_drill_id == 0 {
            return Err(backup_error(
                "restore evidence restore drill id must not be zero",
            ));
        }
        if self.drill_epoch == 0 {
            return Err(backup_error(
                "restore evidence drill epoch must not be zero",
            ));
        }
        if self.target_lsn_verified == 0 {
            return Err(backup_error(
                "restore evidence target LSN verified must not be zero",
            ));
        }
        if self.drill_digest == [0u8; 32] {
            return Err(backup_error(
                "restore evidence drill digest must not be all-zero",
            ));
        }
        Ok(())
    }

    /// Validate that restore evidence was produced for this exact backup
    /// manifest and that the verified target LSN is within the manifest's PITR
    /// boundary.
    pub fn validate_for_manifest<L: BackupLsn>(
        &self,
        manifest: &BackupManifest<L>,
    ) -> BackupResult<()> {
        self.validate()?;
        if self.backup_id != manifest.backup_id {
            return Err(backup_error(
                "restore evidence backup id must match backup manifest",
            ));
        }
        if self.manifest_crc != manifest.manifest_crc {
            return Err(backup_error(
                "restore evidence manifest CRC must match backup manifest",
            ));
        }
        if self.target_lsn_verified < manifest.earliest_pitr_target().get()
            || self.target_lsn_verified > manifest.latest_pitr_target().get()
        {
            return Err(backup_error(
                "restore evidence target LSN must be within backup PITR range",
            ));
        }
        Ok(())
    }
}
