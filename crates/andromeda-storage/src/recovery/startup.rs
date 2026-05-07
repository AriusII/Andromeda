//! Explicit startup-mode model for `FastStart`, `SafeStart`, and
//! `ForensicStart`.
//!
//! # Doctrine
//!
//! * **RAM is never truth.** Every accepted startup decision must point at a
//!   mounted cold snapshot plus a durable WAL prefix. A startup attempt that
//!   only carries hot/in-memory evidence is rejected, regardless of mode.
//! * **Reconstructible truth = cold snapshot + durable WAL.** The
//!   [`StartupEvidence`] struct captures only those two sources. Its fields
//!   are projections of the validated [`crate::DatabaseManifest`] and the
//!   durable WAL scan; nothing here may be populated from RAM-only state.
//! * **Recovery / startup decisions must be auditable.** Every
//!   [`StartupDecision`] carries the mode it was taken under, the evidence
//!   it observed, and either an [`StartupAcceptance`] proof or a
//!   [`StartupRejectionReason`]. The [`StartupAuditProjection`] returned by
//!   [`StartupDecision::audit_projection`] is the structured shape that
//!   the observer plane is expected to emit alongside
//!   [`andromeda_observe::RecoveryTrace`].
//! * **Visible commit requires durable WAL evidence.** This module never
//!   "promotes" or replays anything; it only classifies what is acceptable
//!   to start from. Replay itself is owned by [`crate::RecoveryPlan`] /
//!   [`crate::ConceptualRedoPlan`].
//! * **`ForensicStart` preserves forensic report evidence without mutating
//!   truth.** When forensic mode is selected, the decision flips
//!   `replay_allowed` to `false`: the database may be inspected, but the
//!   startup path may not apply WAL records on top of the cold snapshot.

mod classification;
mod decision;
mod evidence;
mod outcome;

pub use classification::decide_startup;
pub use decision::{StartupAuditProjection, StartupDecision};
pub use evidence::{ObservedBoundary, StartupEvidence};
pub use outcome::{StartupAcceptance, StartupOutcome, StartupRejectionReason};

#[cfg(test)]
mod tests;
