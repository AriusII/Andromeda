use super::{
    error::BackupValidationError,
    plan::BackupManifest,
    primitives::{BackupLsn, Lsn},
    restore_evidence::RestoreEvidence,
};

/// Returned by `RecoverableGate::attach_restore_evidence` when evidence has
/// already been attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlreadyFinalizedError;

impl core::fmt::Display for AlreadyFinalizedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("backup gate already finalized: restore evidence already attached")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoverableGateError {
    AlreadyFinalized(AlreadyFinalizedError),
    InvalidEvidence(BackupValidationError),
}

impl core::fmt::Display for RecoverableGateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::AlreadyFinalized(error) => error.fmt(f),
            Self::InvalidEvidence(error) => error.fmt(f),
        }
    }
}

/// C5 fence: a `BackupManifest` may not be declared recoverable without valid
/// `RestoreEvidence` from a successful restore drill.
///
/// `RecoverableGate` wraps a manifest and tracks whether validated restore
/// evidence has been attached.  `is_recoverable()` returns `true` only when
/// evidence is present **and** `evidence.validate()` passes.
///
/// The gate is finalised by consuming `attach_restore_evidence()`.  A second
/// call returns `Err(AlreadyFinalizedError)` to prevent double-attachment.
#[derive(Debug, Clone)]
pub struct RecoverableGate<L = Lsn> {
    manifest: BackupManifest<L>,
    restore_evidence: Option<RestoreEvidence>,
}

impl<L: BackupLsn> RecoverableGate<L> {
    /// Create a new gate wrapping `manifest` with no evidence attached.
    pub fn new(manifest: BackupManifest<L>) -> Self {
        Self {
            manifest,
            restore_evidence: None,
        }
    }

    /// Attach restore evidence, consuming `self` and returning the updated gate.
    ///
    /// Returns `Err(AlreadyFinalizedError)` if evidence is already attached;
    /// the original gate is lost on that path (fail-closed).
    pub fn attach_restore_evidence(
        self,
        evidence: RestoreEvidence,
    ) -> Result<Self, RecoverableGateError> {
        if self.restore_evidence.is_some() {
            return Err(RecoverableGateError::AlreadyFinalized(
                AlreadyFinalizedError,
            ));
        }
        evidence
            .validate_for_manifest(&self.manifest)
            .map_err(RecoverableGateError::InvalidEvidence)?;
        Ok(Self {
            manifest: self.manifest,
            restore_evidence: Some(evidence),
        })
    }

    /// Returns `true` iff valid restore evidence is attached.
    ///
    /// Fail-closed: returns `false` if evidence is absent or if
    /// `evidence.validate()` would fail.
    pub fn is_recoverable(&self) -> bool {
        match &self.restore_evidence {
            Some(ev) => ev.validate_for_manifest(&self.manifest).is_ok(),
            None => false,
        }
    }

    /// Access the wrapped manifest (read-only).
    pub fn manifest(&self) -> &BackupManifest<L> {
        &self.manifest
    }

    /// Access the restore evidence if attached (read-only).
    pub fn restore_evidence(&self) -> Option<&RestoreEvidence> {
        self.restore_evidence.as_ref()
    }
}
