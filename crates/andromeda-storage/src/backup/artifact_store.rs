//! Compatibility facade for file-backed backup artifacts.
//!
//! Artifact persistence and manifest codecs now live in `andromeda-backup`.

pub use andromeda_backup::{
    BackupArtifactManifestRecord, BackupArtifactWriteReport, BackupWalArchiveEvidence,
    FileBackedBackupArtifactStore,
};
