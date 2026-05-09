//! F6 Restore and PITR Execution Plan — deterministic restore orchestration.
//!
//! This module defines the decision model for restoring from a durable backup manifest
//! to a point-in-time (PITR) using WAL replay. All functions are pure (no I/O, no async).
//!
//! # Execution Model
//!
//! 1. **Validate manifest** — signature, WAL bounds, snapshot identity
//! 2. **Select PITR target LSN** — app-driven (not automatic)
//! 3. **Plan replay segments** — identify WAL segments needed to reach PITR LSN
//! 4. **Validate contiguity** — no gaps in LSN chain
//! 5. **Bind audit trace** — trace ID, backup ID, recovery stage
//! 6. **Execute replay** — (deferred to async; plan only)
//!
//! # Design Principles
//!
//! - **No Automatic Decisions**: PITR LSN is always app-selected; no "latest" default
//! - **Forensic Start Support**: ForensicStart mode collects corruption signals without repair
//! - **Durable Validation**: All inputs derive from backup manifest + WAL archive, not hot memory
//! - **Audit Trail Binding**: Each restore is traced with immutable receipt
//! - **Separation of Concerns**: Manifest validation (F5) vs restore planning (F6)

mod checksum;
mod error;
mod preflight;
mod replay_plan;
mod types;
mod validation;

pub use checksum::compute_restore_checksum;
pub use preflight::validate_restore_artifact_preflight;
pub use replay_plan::{WalSegmentToReplay, plan_replay_segments};
pub use types::{
    RecoveryStage, RestoreArtifactPreflight, RestoreAuditTrace, RestoreBackupManifest,
    RestoreCompletion, RestoreOrchestration, RestoreValidationPolicy,
};
pub use validation::validate_restore_prerequisites;

#[cfg(test)]
mod tests;
