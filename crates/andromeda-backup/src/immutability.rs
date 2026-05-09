use crate::error::{BackupResult, backup_error};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Immutability guard for backup artifacts.
///
/// Once created and published, a backup artifact must never be modified.
/// This type enforces immutability by rejecting any write operations to marked artifacts.
#[derive(Debug, Clone)]
pub struct BackupArtifactImmutabilityGuard {
    is_immutable: Arc<AtomicBool>,
}

impl BackupArtifactImmutabilityGuard {
    /// Create a new mutable artifact guard.
    pub fn new_mutable() -> Self {
        Self {
            is_immutable: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Mark this artifact as immutable.
    /// After this call, all write operations will be rejected.
    pub fn mark_immutable(&self) {
        self.is_immutable.store(true, Ordering::Release);
    }

    /// Check if this artifact is immutable.
    pub fn is_immutable(&self) -> bool {
        self.is_immutable.load(Ordering::Acquire)
    }

    /// Validate that a write operation is permitted.
    /// Returns an error if the artifact is immutable.
    pub fn validate_write_allowed(&self) -> BackupResult<()> {
        if self.is_immutable() {
            return Err(backup_error(
                "backup artifact write rejected: artifact is immutable after creation",
            ));
        }
        Ok(())
    }
}

/// Wrapper for an immutable backup artifact.
/// This type can only be created after immutability is proven.
#[derive(Debug, Clone)]
pub struct ImmutableBackupArtifact {
    guard: BackupArtifactImmutabilityGuard,
}

impl ImmutableBackupArtifact {
    /// Create an immutable artifact guard (proves artifact is immutable).
    pub fn create(guard: BackupArtifactImmutabilityGuard) -> BackupResult<Self> {
        guard.mark_immutable();
        Ok(Self { guard })
    }

    /// Validate that this artifact is immutable.
    pub fn validate(&self) -> BackupResult<()> {
        if !self.guard.is_immutable() {
            return Err(backup_error(
                "immutable backup artifact failed validation: guard reports mutable",
            ));
        }
        Ok(())
    }

    /// Attempt a write operation against this immutable artifact.
    /// Always fails with ImmutableWriteRejected.
    pub fn reject_write(&self) -> BackupResult<()> {
        self.guard.validate_write_allowed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immutability_guard_starts_mutable() {
        let guard = BackupArtifactImmutabilityGuard::new_mutable();
        assert!(!guard.is_immutable());
        assert!(guard.validate_write_allowed().is_ok());
    }

    #[test]
    fn immutability_guard_rejects_writes_after_mark_immutable() {
        let guard = BackupArtifactImmutabilityGuard::new_mutable();
        guard.mark_immutable();
        assert!(guard.is_immutable());
        assert!(guard.validate_write_allowed().is_err());
    }

    #[test]
    fn immutable_artifact_creation_marks_guard_immutable() {
        let guard = BackupArtifactImmutabilityGuard::new_mutable();
        let artifact = ImmutableBackupArtifact::create(guard.clone()).unwrap();
        assert!(guard.is_immutable());
        artifact.validate().unwrap();
    }

    #[test]
    fn immutable_artifact_rejects_write_operations() {
        let guard = BackupArtifactImmutabilityGuard::new_mutable();
        let artifact = ImmutableBackupArtifact::create(guard).unwrap();
        let result = artifact.reject_write();
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("immutable"));
    }

    #[test]
    fn immutable_artifact_gate_proves_immutability() {
        let guard = BackupArtifactImmutabilityGuard::new_mutable();
        assert!(!guard.is_immutable());

        let artifact = ImmutableBackupArtifact::create(guard.clone()).unwrap();
        assert!(guard.is_immutable());

        // Prove immutability through the artifact
        artifact.validate().unwrap();

        // Original guard also reflects immutability
        assert!(guard.is_immutable());
    }
}
