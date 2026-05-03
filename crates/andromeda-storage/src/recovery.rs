use andromeda_core::AndromedaResult;
use andromeda_observe::TraceId;

use crate::{DatabaseManifest, Lsn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupMode {
    FastStart,
    SafeStart,
    ForensicStart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryPlan {
    pub startup_mode: StartupMode,
    pub mounted_snapshot_id: u64,
    pub redo_from_lsn: Lsn,
    pub discard_incomplete_transactions: bool,
}

impl RecoveryPlan {
    pub fn from_manifest(
        manifest: &DatabaseManifest,
        startup_mode: StartupMode,
    ) -> AndromedaResult<Self> {
        manifest.validate()?;

        Ok(Self {
            startup_mode,
            mounted_snapshot_id: manifest.snapshot_id,
            redo_from_lsn: manifest.required_wal_start_lsn,
            discard_incomplete_transactions: true,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryTrace {
    pub trace_id: TraceId,
    pub startup_mode: StartupMode,
    pub replay_start_lsn: Lsn,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_keeps_snapshot_plus_wal_anchor() {
        let manifest = DatabaseManifest {
            database_id: 1,
            manifest_version: 2,
            snapshot_id: 3,
            base_checkpoint_lsn: Lsn::new(100),
            required_wal_start_lsn: Lsn::new(101),
            previous_manifest_hash: [0; 32],
            manifest_crc: 99,
        };

        assert!(manifest.validate().is_ok());
        assert_eq!(
            RecoveryPlan::from_manifest(&manifest, StartupMode::SafeStart)
                .unwrap()
                .redo_from_lsn,
            Lsn::new(101)
        );
    }
}
