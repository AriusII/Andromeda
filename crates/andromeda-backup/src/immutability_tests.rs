/// Phase 7 Durability Gate Tests: Backup Immutability
///
/// These tests validate C5 invariants for backup artifact immutability:
/// - Backup artifacts are immutable after creation
/// - Writes to immutable artifacts are rejected with clear error
/// - Immutability cannot be reversed
/// - Guards prove immutability through multiple code paths
use crate::immutability::BackupArtifactImmutabilityGuard;

#[test]
fn backup_artifact_immutability_gate_prevents_modification() {
    // Create a mutable artifact
    let guard = BackupArtifactImmutabilityGuard::new_mutable();

    // Initially allows writes
    assert!(guard.validate_write_allowed().is_ok());

    // Mark immutable (simulates successful backup creation)
    guard.mark_immutable();

    // Now rejects all writes
    let write_result = guard.validate_write_allowed();
    assert!(write_result.is_err());
    assert!(write_result.unwrap_err().message().contains("immutable"));
}

#[test]
fn backup_artifact_immutability_gate_is_irreversible() {
    let guard = BackupArtifactImmutabilityGuard::new_mutable();

    // Mark immutable
    guard.mark_immutable();
    assert!(guard.is_immutable());

    // Attempt to mark mutable again (impossible - no such API exists)
    // Verify immutability persists
    assert!(guard.is_immutable());
    assert!(guard.validate_write_allowed().is_err());
}

#[test]
fn backup_artifact_immutability_gate_shared_across_clones() {
    let guard1 = BackupArtifactImmutabilityGuard::new_mutable();
    let guard2 = guard1.clone();

    // Mark immutable via guard1
    guard1.mark_immutable();

    // Immutability visible via guard2 (shared state)
    assert!(guard2.is_immutable());
    assert!(guard2.validate_write_allowed().is_err());
}

#[test]
fn backup_artifact_immutability_gate_error_message_is_clear() {
    let guard = BackupArtifactImmutabilityGuard::new_mutable();
    guard.mark_immutable();

    let err = guard.validate_write_allowed().unwrap_err();
    assert_eq!(
        err.message(),
        "backup artifact write rejected: artifact is immutable after creation"
    );
}

#[test]
fn backup_immutable_artifact_wrapper_proves_immutability() {
    use crate::immutability::ImmutableBackupArtifact;

    let guard = BackupArtifactImmutabilityGuard::new_mutable();

    // Create immutable artifact wrapper
    let artifact = ImmutableBackupArtifact::create(guard.clone()).unwrap();

    // Immutability proven through wrapper
    artifact.validate().unwrap();

    // Original guard is also immutable
    assert!(guard.is_immutable());
}

#[test]
fn backup_immutable_artifact_wrapper_rejects_write_attempts() {
    use crate::immutability::ImmutableBackupArtifact;

    let guard = BackupArtifactImmutabilityGuard::new_mutable();
    let artifact = ImmutableBackupArtifact::create(guard).unwrap();

    // Attempt to write against immutable artifact
    let write_result = artifact.reject_write();
    assert!(write_result.is_err());
    assert!(write_result.unwrap_err().message().contains("immutable"));
}
