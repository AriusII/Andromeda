use super::error::{BackupResult, backup_error};

/// Durable backup retention policy persisted in `BackupManifest` v4.
///
/// C5 fence: all fields must be nonzero and `pitr_window_start_lsn < pitr_window_end_lsn`.
/// No struct is persisted to disk — the codec uses explicit LE push functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupRetentionPolicy {
    /// Inclusive start LSN of the PITR window covered by this backup.
    pub pitr_window_start_lsn: u64,
    /// Inclusive end LSN of the PITR window covered by this backup.
    pub pitr_window_end_lsn: u64,
    /// Wall-clock epoch (seconds since Unix epoch) after which this backup may be expired.
    pub expires_at_epoch: u64,
}

impl BackupRetentionPolicy {
    /// Validates retention policy fields are all nonzero and window is ordered.
    ///
    /// Fails closed: zero value in any field is a hard error.
    pub fn validate(&self) -> BackupResult<()> {
        if self.pitr_window_start_lsn == 0 {
            return Err(backup_error(
                "backup retention policy PITR window start LSN must not be zero",
            ));
        }
        if self.pitr_window_end_lsn == 0 {
            return Err(backup_error(
                "backup retention policy PITR window end LSN must not be zero",
            ));
        }
        if self.expires_at_epoch == 0 {
            return Err(backup_error(
                "backup retention policy expiry epoch must not be zero",
            ));
        }
        if self.pitr_window_start_lsn >= self.pitr_window_end_lsn {
            return Err(backup_error(
                "backup retention policy PITR window start LSN must precede end LSN",
            ));
        }
        Ok(())
    }

    /// Returns whether a WAL artifact range is required to satisfy PITR.
    ///
    /// Fails closed when the policy or artifact range is invalid.
    pub fn requires_pitr_artifact(
        &self,
        artifact_start_lsn: u64,
        artifact_end_lsn: u64,
    ) -> BackupResult<bool> {
        self.validate()?;
        if artifact_start_lsn == 0 {
            return Err(backup_error(
                "backup retention predicate artifact start LSN must not be zero",
            ));
        }
        if artifact_end_lsn == 0 {
            return Err(backup_error(
                "backup retention predicate artifact end LSN must not be zero",
            ));
        }
        if artifact_start_lsn > artifact_end_lsn {
            return Err(backup_error(
                "backup retention predicate artifact start LSN must not exceed end LSN",
            ));
        }

        Ok(artifact_end_lsn >= self.pitr_window_start_lsn
            && artifact_start_lsn <= self.pitr_window_end_lsn)
    }
}
