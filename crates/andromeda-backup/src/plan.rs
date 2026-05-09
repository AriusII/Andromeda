use super::{
    artifacts::BackupWalSegmentArtifact,
    error::{BackupResult, backup_error},
    primitives::BackupLsn,
    types::{BackupId, ColdSnapshotBoundary, WalArchiveRange},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupManifest<L = super::Lsn> {
    pub backup_id: BackupId,
    pub database_id: u64,
    pub created_epoch: u64,
    pub snapshot: ColdSnapshotBoundary<L>,
    pub wal_archive: WalArchiveRange<L>,
    pub manifest_crc: u32,
}

impl<L: BackupLsn> BackupManifest<L> {
    pub fn validate(&self) -> BackupResult<()> {
        if self.backup_id.is_zero() {
            return Err(backup_error("backup id must not be zero"));
        }
        if self.database_id == 0 {
            return Err(backup_error("backup database id must not be zero"));
        }
        if self.created_epoch == 0 {
            return Err(backup_error("backup created epoch must not be zero"));
        }
        if self.manifest_crc == 0 {
            return Err(backup_error("backup manifest CRC must not be zero"));
        }
        self.snapshot.validate()?;
        self.wal_archive.validate()?;

        if self.wal_archive.end_inclusive < self.snapshot.base_checkpoint_lsn {
            return Err(backup_error(
                "backup WAL archive end must not precede snapshot base checkpoint LSN",
            ));
        }

        Ok(())
    }

    pub const fn earliest_pitr_target(&self) -> L {
        self.snapshot.base_checkpoint_lsn
    }

    pub const fn latest_pitr_target(&self) -> L {
        self.wal_archive.end_inclusive
    }
}

pub fn validate_wal_segment_chain<L: BackupLsn>(
    archive: WalArchiveRange<L>,
    segments: &[BackupWalSegmentArtifact<L>],
) -> BackupResult<()> {
    archive.validate()?;
    let Some(first) = segments.first() else {
        return Err(backup_error(
            "backup WAL archive must list at least one segment",
        ));
    };
    if first.first_lsn != archive.start {
        return Err(backup_error(
            "backup WAL first segment must start at archive start LSN",
        ));
    }

    let mut expected_first = archive.start;
    let mut previous_last = None;
    for (index, segment) in segments.iter().enumerate() {
        segment.validate()?;
        if segment.first_lsn != expected_first {
            return Err(backup_error("backup WAL segment chain has an LSN gap"));
        }
        if segment.base_previous_lsn != previous_last {
            return Err(backup_error(
                "backup WAL segment base previous LSN does not chain to prior segment",
            ));
        }
        if segments[..index]
            .iter()
            .any(|previous| previous.segment_id == segment.segment_id)
        {
            return Err(backup_error("backup WAL segment ids must not repeat"));
        }
        previous_last = Some(segment.last_lsn);
        expected_first = segment.last_lsn.try_next()?;
    }

    let last = segments
        .last()
        .ok_or_else(|| backup_error("backup WAL archive must list at least one segment"))?;
    if last.last_lsn != archive.end_inclusive {
        return Err(backup_error(
            "backup WAL last segment must end at archive end LSN",
        ));
    }
    Ok(())
}
