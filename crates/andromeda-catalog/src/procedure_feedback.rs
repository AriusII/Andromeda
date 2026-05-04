//! V0 evidence-only Procedure feedback scaffold.
//!
//! **Status: SCAFFOLD ONLY.**  This module does not run plans, does not
//! compile procedures, does not own a runtime cache, and does **not**
//! make optimizer decisions.  It only models the bounded shape of an
//! observed-invocation-outcome record that a future Procedure Store /
//! optimizer may consume *as advisory input*.  Decisions remain
//! deterministic and traceable; runtime observations are never
//! silently promoted to truth.
//!
//! ## Doctrine pinned by this module
//!
//! 1. **Advisory by construction.**  Every [`ProcedureFeedback`] reports
//!    `is_authoritative() == false`.  There is no constructor or setter
//!    that can flip the flag.  Optimizer code that ever needs to know
//!    "may I treat this as binding?" gets a single, unambiguous,
//!    type-level `false`.
//! 2. **Deterministic digest.**  [`ProcedureFeedback::digest`] is a
//!    SHA-256 over a domain-tagged, byte-tagged, little-endian encoding
//!    of every field.  Equal records produce equal digests on every
//!    node, regardless of build target.  Plan / cache key digest is
//!    folded in with explicit `present` / `absent` discriminants so a
//!    missing plan key cannot collide with an all-zero plan key.
//! 3. **Mandatory observed window.**  Every record carries a
//!    [`ValidityWindow`] (`issued_at < expires_at`, `expires_at != 0`)
//!    so consumers must apply expiry explicitly.  No silent "valid
//!    forever" path.
//! 4. **Explicit version targeting.**  Every record names the
//!    `StatsVersion` it was observed against, so a feedback gathered
//!    under one stats snapshot can never silently apply to a different
//!    snapshot.
//! 5. **Bounded completion evidence.**  [`CompletionStatus`] is a
//!    closed enum with stable tag bytes.  `Committed` outcomes require
//!    non-zero durable LSN evidence, mirroring the
//!    `CompletionEmittedTrace` invariant in `andromeda-observe`.
//! 6. **Bounded retention.**  [`InMemoryProcedureFeedbackStore`] has a
//!    hard capacity and deterministic eviction order
//!    (`(issued_at, feedback_id)` ascending).  No unbounded growth, no
//!    wall-clock dependence, no SQL surface.
//! 7. **Conflict handling is deterministic.**  Recording the same
//!    [`FeedbackId`] with a *different* digest is a [`Conflict`]
//!    rejection.  Recording the same id with the *same* digest is a
//!    [`Duplicate`] no-op.  There is no last-write-wins behaviour.
//!
//! ## What this module deliberately does NOT do
//!
//! - It does not select, score, or rank plans.
//! - It does not own protocol wire formats.
//! - It does not interpret SQL.  The project remains Procedure-only.
//! - It does not let feedback become authoritative.  Even a perfectly
//!   matched, freshly issued record is advisory input to the optimizer.
//! - It does not depend on `andromeda-observe`; the `EventSink` /
//!   `CompletionEmittedTrace` types in that crate are the *source* of
//!   the evidence values, but conversion is performed at the call site
//!   so this crate stays free of a runtime-trace dependency.

use andromeda_core::{EngineTimestamp, ProcedureId};

use crate::contracts::StatsVersion;
use crate::digest::Sha256;
use crate::scenario_evidence::{ScenarioEvidenceError, ValidityWindow};

/// Domain tag absorbed at the start of every procedure-feedback digest.
const PROCEDURE_FEEDBACK_DOMAIN: &[u8] = b"andromeda.procedure_feedback.v0";

/// Bounded completion-status taxonomy mirroring the terminal outcomes
/// observable from an invocation completion trace.  The variants are
/// closed; adding one is a doctrine change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CompletionStatus {
    /// The invocation committed durably.  Requires non-zero
    /// `durable_lsn` evidence.
    Committed,
    /// The invocation rolled back cleanly.  No durable LSN required.
    RolledBack,
    /// The invocation failed (contract / runtime error).
    Failed,
    /// The invocation was aborted (e.g. admission / surface gate).
    Aborted,
}

impl CompletionStatus {
    /// Stable tag byte folded into the feedback digest.  Reordering or
    /// reusing tag bytes is a doctrine change.
    pub const fn as_tag(self) -> u8 {
        match self {
            CompletionStatus::Committed => 0x01,
            CompletionStatus::RolledBack => 0x02,
            CompletionStatus::Failed => 0x03,
            CompletionStatus::Aborted => 0x04,
        }
    }

    /// Total number of variants.  Asserted by tests so accidental growth
    /// is flagged immediately.
    pub const VARIANT_COUNT: usize = 4;
}

/// Bounded completion evidence drawn from an invocation completion
/// trace.  Optionality is encoded with explicit `Option` so the digest
/// distinguishes "absent" from "present-but-zero".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompletionEvidence {
    pub status: CompletionStatus,
    /// Wire-aligned completion code, when emitted.  Sourced from
    /// `CompletionEmittedTrace::completion_code`.
    pub completion_code: Option<u32>,
    /// Producer-reported row count, when known.  No semantic
    /// interpretation here; consumers must treat as advisory.
    pub row_count: Option<u64>,
    /// Durable LSN evidence.  When `status == Committed` this MUST be
    /// `Some(non_zero)`; otherwise it must not be `Some(0)`.  This
    /// mirrors the `CompletionEmittedTrace` invariant in
    /// `andromeda-observe`.
    pub durable_lsn: Option<u64>,
}

impl CompletionEvidence {
    /// Validate the bounded structural invariants.  No clamping; bad
    /// inputs are bugs and must be reported as such.
    pub fn validate(&self) -> Result<(), ProcedureFeedbackError> {
        // Reject Some(0) durable LSN regardless of status: a zero LSN
        // is never durable evidence.
        if let Some(0) = self.durable_lsn {
            return Err(ProcedureFeedbackError::DurableLsnZero);
        }
        // Committed outcomes require non-zero durable LSN evidence.
        if let CompletionStatus::Committed = self.status {
            match self.durable_lsn {
                Some(lsn) if lsn != 0 => {}
                _ => return Err(ProcedureFeedbackError::CommittedRequiresDurableLsn),
            }
        }
        Ok(())
    }
}

/// Stable feedback-record identity.  Non-zero so a "no feedback"
/// sentinel cannot accidentally produce a valid digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FeedbackId(u64);

impl FeedbackId {
    /// Construct a `FeedbackId`.  Returns `None` for the reserved zero
    /// value.
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// V0 evidence-only feedback record for an observed invocation outcome.
///
/// Construct via [`ProcedureFeedback::new`], which performs every
/// bounded check in a single place.  All fields are private so external
/// callers cannot bypass validation by direct struct-literal
/// construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcedureFeedback {
    feedback_id: FeedbackId,
    procedure_id: ProcedureId,
    /// Optional plan-cache key digest (see
    /// `crate::plan_cache::PlanCacheKey::digest`).  When absent the
    /// observation could not be attributed to a specific plan.
    plan_cache_key_digest: Option<[u8; 32]>,
    stats_version: StatsVersion,
    completion: CompletionEvidence,
    observed_window: ValidityWindow,
}

impl ProcedureFeedback {
    /// Construct a feedback record after running every bounded check.
    ///
    /// On success the resulting record is guaranteed to:
    /// - have a non-zero feedback id, procedure id, and stats version,
    /// - carry an `observed_window` with `issued_at < expires_at` and
    ///   `expires_at > 0`,
    /// - carry valid completion evidence (see
    ///   [`CompletionEvidence::validate`]),
    /// - carry a non-all-zero plan-cache key digest whenever one is
    ///   supplied.
    pub fn new(
        feedback_id: FeedbackId,
        procedure_id: ProcedureId,
        plan_cache_key_digest: Option<[u8; 32]>,
        stats_version: StatsVersion,
        completion: CompletionEvidence,
        observed_window: ValidityWindow,
    ) -> Result<Self, ProcedureFeedbackError> {
        if procedure_id.get() == 0 {
            return Err(ProcedureFeedbackError::ProcedureIdZero);
        }
        if stats_version.get() == 0 {
            return Err(ProcedureFeedbackError::StatsVersionZero);
        }
        if let Some(digest) = plan_cache_key_digest {
            if digest == [0u8; 32] {
                return Err(ProcedureFeedbackError::PlanCacheKeyDigestZero);
            }
        }
        completion.validate()?;
        Ok(Self {
            feedback_id,
            procedure_id,
            plan_cache_key_digest,
            stats_version,
            completion,
            observed_window,
        })
    }

    pub const fn feedback_id(&self) -> FeedbackId {
        self.feedback_id
    }

    pub const fn procedure_id(&self) -> ProcedureId {
        self.procedure_id
    }

    pub const fn plan_cache_key_digest(&self) -> Option<[u8; 32]> {
        self.plan_cache_key_digest
    }

    pub const fn stats_version(&self) -> StatsVersion {
        self.stats_version
    }

    pub const fn completion(&self) -> &CompletionEvidence {
        &self.completion
    }

    pub const fn observed_window(&self) -> ValidityWindow {
        self.observed_window
    }

    /// **Doctrine invariant: procedure feedback is never authoritative.**
    /// This always returns `false`.  It exists so optimizer-side code
    /// can express the doctrine at the type level
    /// (`assert!(!fb.is_authoritative())`) without conditional logic.
    pub const fn is_authoritative(&self) -> bool {
        false
    }

    /// `true` when `now >= expires_at`.
    pub fn is_expired_at(&self, now: EngineTimestamp) -> bool {
        self.observed_window.is_expired_at(now)
    }

    /// Validate the record for consumption *at* `now`.  Returns:
    /// - `Err(NotYetValid)` if the record is observed in the future,
    /// - `Err(Expired)` if the record has expired,
    /// - `Ok(())` otherwise.
    ///
    /// Optimizer code is required to call this (or
    /// [`Self::is_expired_at`]) before consuming feedback.  Calling
    /// neither is a doctrine violation: there is no silent bypass.
    pub fn validate_for_use_at(&self, now: EngineTimestamp) -> Result<(), ScenarioEvidenceError> {
        // Re-use the closed `ScenarioEvidenceError::{NotYetValid,
        // Expired}` variants so consumers handle one expiry vocabulary
        // across all evidence kinds.
        if self.observed_window.is_not_yet_valid_at(now) {
            return Err(ScenarioEvidenceError::NotYetValid);
        }
        if self.observed_window.is_expired_at(now) {
            return Err(ScenarioEvidenceError::Expired);
        }
        Ok(())
    }

    /// Compute a deterministic 32-byte digest over every field.
    ///
    /// Equal records produce equal digests; differing records produce
    /// differing digests with overwhelming probability (SHA-256).  Tag
    /// bytes and little-endian widths make the encoding stable across
    /// architectures.
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(PROCEDURE_FEEDBACK_DOMAIN);

        hasher.update(&[0xF0]);
        hasher.update(&self.feedback_id.get().to_le_bytes());

        hasher.update(&[0xF1]);
        hasher.update(&self.procedure_id.get().to_le_bytes());

        hasher.update(&[0xF2]);
        match self.plan_cache_key_digest {
            Some(d) => {
                hasher.update(&[0x01]);
                hasher.update(&d);
            }
            None => {
                hasher.update(&[0x00]);
                hasher.update(&[0u8; 32]);
            }
        }

        hasher.update(&[0xF3]);
        hasher.update(&self.stats_version.get().to_le_bytes());

        hasher.update(&[0xF4, self.completion.status.as_tag()]);
        hasher.update(&[0xF5]);
        match self.completion.completion_code {
            Some(c) => {
                hasher.update(&[0x01]);
                hasher.update(&c.to_le_bytes());
            }
            None => {
                hasher.update(&[0x00]);
                hasher.update(&[0u8; 4]);
            }
        }
        hasher.update(&[0xF6]);
        match self.completion.row_count {
            Some(r) => {
                hasher.update(&[0x01]);
                hasher.update(&r.to_le_bytes());
            }
            None => {
                hasher.update(&[0x00]);
                hasher.update(&[0u8; 8]);
            }
        }
        hasher.update(&[0xF7]);
        match self.completion.durable_lsn {
            Some(l) => {
                hasher.update(&[0x01]);
                hasher.update(&l.to_le_bytes());
            }
            None => {
                hasher.update(&[0x00]);
                hasher.update(&[0u8; 8]);
            }
        }

        hasher.update(&[0xF8]);
        hasher.update(
            &self
                .observed_window
                .issued_at()
                .as_unix_millis()
                .to_le_bytes(),
        );
        hasher.update(&[0xF9]);
        hasher.update(
            &self
                .observed_window
                .expires_at()
                .as_unix_millis()
                .to_le_bytes(),
        );

        // Authoritative flag is folded in as a constant `0` so a future
        // attempt to add an "authoritative" variant would change the
        // digest layout and break compatibility loudly.
        hasher.update(&[0xFA, 0x00]);

        hasher.finalize()
    }
}

/// Closed set of reasons a [`ProcedureFeedback`] may be rejected at
/// construction time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureFeedbackError {
    /// `procedure_id` was zero.
    ProcedureIdZero,
    /// `stats_version` was zero.
    StatsVersionZero,
    /// `plan_cache_key_digest` was supplied but all-zero.
    PlanCacheKeyDigestZero,
    /// A `Some(0)` durable LSN was supplied; zero is never durable
    /// evidence.
    DurableLsnZero,
    /// `CompletionStatus::Committed` was supplied without a non-zero
    /// durable LSN.
    CommittedRequiresDurableLsn,
}

impl core::fmt::Display for ProcedureFeedbackError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProcedureFeedbackError::ProcedureIdZero => {
                f.write_str("ProcedureFeedback.procedure_id must be non-zero")
            }
            ProcedureFeedbackError::StatsVersionZero => {
                f.write_str("ProcedureFeedback.stats_version must be non-zero")
            }
            ProcedureFeedbackError::PlanCacheKeyDigestZero => f.write_str(
                "ProcedureFeedback.plan_cache_key_digest, when present, must be non-zero",
            ),
            ProcedureFeedbackError::DurableLsnZero => {
                f.write_str("CompletionEvidence.durable_lsn, when present, must be non-zero")
            }
            ProcedureFeedbackError::CommittedRequiresDurableLsn => {
                f.write_str("CompletionStatus::Committed requires non-zero durable LSN evidence")
            }
        }
    }
}

/// Outcome of recording a feedback record into a store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOutcome {
    /// The record was newly stored.
    Stored,
    /// A byte-identical record (same `feedback_id` and same digest)
    /// already exists; this call was a deterministic no-op.
    Duplicate,
    /// The store was at capacity.  The lowest-ranked existing record
    /// (smallest `(issued_at, feedback_id)`) was evicted to make room
    /// and the new record was stored.
    StoredAfterEviction,
}

/// Closed set of reasons a feedback record may be rejected at store
/// time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureFeedbackStoreError {
    /// A record with the same `feedback_id` but a different digest
    /// already exists.  No silent overwrite.
    Conflict { feedback_id: FeedbackId },
    /// The store was constructed with capacity 0; nothing can be
    /// stored.
    ZeroCapacity,
}

impl core::fmt::Display for ProcedureFeedbackStoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProcedureFeedbackStoreError::Conflict { feedback_id } => write!(
                f,
                "ProcedureFeedback conflict: id={} already present with a different digest",
                feedback_id.get()
            ),
            ProcedureFeedbackStoreError::ZeroCapacity => {
                f.write_str("ProcedureFeedback store has zero capacity")
            }
        }
    }
}

/// Narrow store surface for advisory feedback.  A future Procedure
/// Store crate can implement this trait without taking a circular
/// dependency on `andromeda-catalog`.
pub trait ProcedureFeedbackStore {
    fn record(
        &mut self,
        feedback: ProcedureFeedback,
    ) -> Result<RecordOutcome, ProcedureFeedbackStoreError>;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return non-expired feedback for `procedure_id` at `now`, in the
    /// store's deterministic order.
    fn iter_for_procedure_at(
        &self,
        procedure_id: ProcedureId,
        now: EngineTimestamp,
    ) -> Vec<ProcedureFeedback>;

    /// Drop every record whose window has expired by `now`.  Returns
    /// the number of records removed.
    fn prune_expired(&mut self, now: EngineTimestamp) -> usize;
}

/// Bounded in-memory implementation.
///
/// Storage is a sorted vector keyed by `(issued_at, feedback_id)` so
/// eviction order is deterministic and reproducible across runs.  The
/// store does **not** consult a wall clock; expiry is always driven by
/// an explicit `now` argument supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InMemoryProcedureFeedbackStore {
    capacity: usize,
    items: Vec<ProcedureFeedback>,
}

impl InMemoryProcedureFeedbackStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            items: Vec::new(),
        }
    }

    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Read-only view in deterministic `(issued_at, feedback_id)`
    /// order.
    pub fn iter(&self) -> impl Iterator<Item = &ProcedureFeedback> {
        self.items.iter()
    }

    fn rank(fb: &ProcedureFeedback) -> (u64, u64) {
        (
            fb.observed_window().issued_at().as_unix_millis(),
            fb.feedback_id().get(),
        )
    }

    fn find_by_id(&self, id: FeedbackId) -> Option<usize> {
        self.items
            .iter()
            .position(|existing| existing.feedback_id() == id)
    }

    fn insert_sorted(&mut self, fb: ProcedureFeedback) {
        let key = Self::rank(&fb);
        let pos = self
            .items
            .binary_search_by(|existing| Self::rank(existing).cmp(&key))
            .unwrap_or_else(|p| p);
        self.items.insert(pos, fb);
    }
}

impl ProcedureFeedbackStore for InMemoryProcedureFeedbackStore {
    fn record(
        &mut self,
        feedback: ProcedureFeedback,
    ) -> Result<RecordOutcome, ProcedureFeedbackStoreError> {
        if self.capacity == 0 {
            return Err(ProcedureFeedbackStoreError::ZeroCapacity);
        }

        if let Some(idx) = self.find_by_id(feedback.feedback_id()) {
            let existing = &self.items[idx];
            if existing.digest() == feedback.digest() {
                return Ok(RecordOutcome::Duplicate);
            }
            return Err(ProcedureFeedbackStoreError::Conflict {
                feedback_id: feedback.feedback_id(),
            });
        }

        let mut evicted = false;
        if self.items.len() >= self.capacity {
            // Deterministic eviction: drop the lowest-ranked record
            // (smallest `issued_at`, then smallest `feedback_id`),
            // which is index 0 in the sorted vector.
            self.items.remove(0);
            evicted = true;
        }
        self.insert_sorted(feedback);
        Ok(if evicted {
            RecordOutcome::StoredAfterEviction
        } else {
            RecordOutcome::Stored
        })
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn iter_for_procedure_at(
        &self,
        procedure_id: ProcedureId,
        now: EngineTimestamp,
    ) -> Vec<ProcedureFeedback> {
        self.items
            .iter()
            .filter(|fb| fb.procedure_id() == procedure_id && fb.validate_for_use_at(now).is_ok())
            .copied()
            .collect()
    }

    fn prune_expired(&mut self, now: EngineTimestamp) -> usize {
        let before = self.items.len();
        self.items.retain(|fb| !fb.is_expired_at(now));
        before - self.items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::ProcedureId;

    fn ts(ms: u64) -> EngineTimestamp {
        EngineTimestamp::from_unix_millis(ms)
    }

    fn window(issued: u64, expires: u64) -> ValidityWindow {
        ValidityWindow::new(ts(issued), ts(expires)).expect("valid window")
    }

    fn committed_completion(lsn: u64) -> CompletionEvidence {
        CompletionEvidence {
            status: CompletionStatus::Committed,
            completion_code: Some(0),
            row_count: Some(7),
            durable_lsn: Some(lsn),
        }
    }

    fn make_feedback(id: u64, issued: u64, expires: u64) -> ProcedureFeedback {
        ProcedureFeedback::new(
            FeedbackId::new(id).expect("non-zero id"),
            ProcedureId::new(11),
            Some([0xAA; 32]),
            StatsVersion::new(5),
            committed_completion(99),
            window(issued, expires),
        )
        .expect("valid feedback")
    }

    #[test]
    fn completion_status_variant_count_is_bounded_and_tags_are_unique() {
        assert_eq!(CompletionStatus::VARIANT_COUNT, 4);
        let tags = [
            CompletionStatus::Committed.as_tag(),
            CompletionStatus::RolledBack.as_tag(),
            CompletionStatus::Failed.as_tag(),
            CompletionStatus::Aborted.as_tag(),
        ];
        let mut sorted = tags;
        sorted.sort_unstable();
        assert!(sorted.windows(2).all(|p| p[0] != p[1]));
    }

    #[test]
    fn feedback_id_rejects_zero() {
        assert!(FeedbackId::new(0).is_none());
        assert_eq!(FeedbackId::new(3).unwrap().get(), 3);
    }

    #[test]
    fn feedback_is_never_authoritative() {
        let fb = make_feedback(1, 10, 100);
        assert!(!fb.is_authoritative());
    }

    #[test]
    fn rejects_zero_procedure_id_stats_and_plan_digest() {
        let bad_proc = ProcedureFeedback::new(
            FeedbackId::new(1).unwrap(),
            ProcedureId::new(0),
            None,
            StatsVersion::new(1),
            committed_completion(1),
            window(1, 2),
        );
        assert_eq!(bad_proc, Err(ProcedureFeedbackError::ProcedureIdZero));

        let bad_stats = ProcedureFeedback::new(
            FeedbackId::new(1).unwrap(),
            ProcedureId::new(1),
            None,
            StatsVersion::new(0),
            committed_completion(1),
            window(1, 2),
        );
        assert_eq!(bad_stats, Err(ProcedureFeedbackError::StatsVersionZero));

        let bad_plan = ProcedureFeedback::new(
            FeedbackId::new(1).unwrap(),
            ProcedureId::new(1),
            Some([0u8; 32]),
            StatsVersion::new(1),
            committed_completion(1),
            window(1, 2),
        );
        assert_eq!(
            bad_plan,
            Err(ProcedureFeedbackError::PlanCacheKeyDigestZero)
        );
    }

    #[test]
    fn completion_evidence_invariants_are_enforced() {
        let zero_lsn = CompletionEvidence {
            status: CompletionStatus::Failed,
            completion_code: None,
            row_count: None,
            durable_lsn: Some(0),
        };
        assert_eq!(
            zero_lsn.validate(),
            Err(ProcedureFeedbackError::DurableLsnZero)
        );

        let committed_no_lsn = CompletionEvidence {
            status: CompletionStatus::Committed,
            completion_code: None,
            row_count: None,
            durable_lsn: None,
        };
        assert_eq!(
            committed_no_lsn.validate(),
            Err(ProcedureFeedbackError::CommittedRequiresDurableLsn)
        );

        let rolled_back = CompletionEvidence {
            status: CompletionStatus::RolledBack,
            completion_code: Some(2),
            row_count: Some(0),
            durable_lsn: None,
        };
        assert!(rolled_back.validate().is_ok());
    }

    #[test]
    fn digest_is_deterministic_and_field_separating() {
        let a = make_feedback(1, 10, 100);
        let b = make_feedback(1, 10, 100);
        assert_eq!(
            a.digest(),
            b.digest(),
            "equal records must have equal digests"
        );

        // Differ in feedback_id
        let other_id = ProcedureFeedback::new(
            FeedbackId::new(2).unwrap(),
            a.procedure_id(),
            a.plan_cache_key_digest(),
            a.stats_version(),
            *a.completion(),
            a.observed_window(),
        )
        .unwrap();
        assert_ne!(a.digest(), other_id.digest());

        // Differ in stats_version
        let other_stats = ProcedureFeedback::new(
            a.feedback_id(),
            a.procedure_id(),
            a.plan_cache_key_digest(),
            StatsVersion::new(99),
            *a.completion(),
            a.observed_window(),
        )
        .unwrap();
        assert_ne!(a.digest(), other_stats.digest());

        // Differ: present-but-different plan key vs none
        let other_plan = ProcedureFeedback::new(
            a.feedback_id(),
            a.procedure_id(),
            Some([0xBB; 32]),
            a.stats_version(),
            *a.completion(),
            a.observed_window(),
        )
        .unwrap();
        let no_plan = ProcedureFeedback::new(
            a.feedback_id(),
            a.procedure_id(),
            None,
            a.stats_version(),
            *a.completion(),
            a.observed_window(),
        )
        .unwrap();
        assert_ne!(a.digest(), other_plan.digest());
        assert_ne!(a.digest(), no_plan.digest());
        assert_ne!(other_plan.digest(), no_plan.digest());

        // Differ in completion details
        let other_completion = ProcedureFeedback::new(
            a.feedback_id(),
            a.procedure_id(),
            a.plan_cache_key_digest(),
            a.stats_version(),
            CompletionEvidence {
                status: CompletionStatus::Failed,
                completion_code: Some(42),
                row_count: None,
                durable_lsn: None,
            },
            a.observed_window(),
        )
        .unwrap();
        assert_ne!(a.digest(), other_completion.digest());
    }

    #[test]
    fn validate_for_use_at_handles_window_boundaries() {
        let fb = make_feedback(1, 10, 100);
        assert_eq!(
            fb.validate_for_use_at(ts(5)),
            Err(ScenarioEvidenceError::NotYetValid)
        );
        assert!(fb.validate_for_use_at(ts(10)).is_ok());
        assert!(fb.validate_for_use_at(ts(99)).is_ok());
        assert_eq!(
            fb.validate_for_use_at(ts(100)),
            Err(ScenarioEvidenceError::Expired)
        );
    }

    #[test]
    fn store_records_and_rejects_zero_capacity() {
        let mut store = InMemoryProcedureFeedbackStore::new(0);
        let fb = make_feedback(1, 10, 100);
        assert_eq!(
            store.record(fb),
            Err(ProcedureFeedbackStoreError::ZeroCapacity)
        );

        let mut store = InMemoryProcedureFeedbackStore::new(4);
        assert_eq!(
            store.record(make_feedback(1, 10, 100)).unwrap(),
            RecordOutcome::Stored
        );
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn store_treats_byte_identical_record_as_idempotent_duplicate() {
        let mut store = InMemoryProcedureFeedbackStore::new(4);
        let fb = make_feedback(1, 10, 100);
        assert_eq!(store.record(fb).unwrap(), RecordOutcome::Stored);
        assert_eq!(store.record(fb).unwrap(), RecordOutcome::Duplicate);
        assert_eq!(store.len(), 1, "duplicate must not grow the store");
    }

    #[test]
    fn store_rejects_conflicting_record_with_same_id_but_different_digest() {
        let mut store = InMemoryProcedureFeedbackStore::new(4);
        store.record(make_feedback(1, 10, 100)).unwrap();

        let conflicting = ProcedureFeedback::new(
            FeedbackId::new(1).unwrap(),
            ProcedureId::new(11),
            Some([0xAA; 32]),
            StatsVersion::new(5),
            CompletionEvidence {
                status: CompletionStatus::Failed,
                completion_code: Some(7),
                row_count: None,
                durable_lsn: None,
            },
            window(10, 100),
        )
        .unwrap();

        let err = store.record(conflicting).unwrap_err();
        assert_eq!(
            err,
            ProcedureFeedbackStoreError::Conflict {
                feedback_id: FeedbackId::new(1).unwrap()
            }
        );
        assert_eq!(store.len(), 1, "conflicts must not mutate the store");
    }

    #[test]
    fn store_evicts_lowest_ranked_when_at_capacity() {
        let mut store = InMemoryProcedureFeedbackStore::new(2);
        store.record(make_feedback(1, 10, 100)).unwrap();
        store.record(make_feedback(2, 20, 100)).unwrap();
        // Newcomer with later `issued_at` triggers eviction of id=1
        // (smallest issued_at).
        let outcome = store.record(make_feedback(3, 30, 100)).unwrap();
        assert_eq!(outcome, RecordOutcome::StoredAfterEviction);
        assert_eq!(store.len(), 2);
        let remaining: Vec<u64> = store.iter().map(|fb| fb.feedback_id().get()).collect();
        assert_eq!(remaining, vec![2, 3]);
    }

    #[test]
    fn store_prune_expired_removes_only_expired_records() {
        let mut store = InMemoryProcedureFeedbackStore::new(8);
        store.record(make_feedback(1, 10, 50)).unwrap();
        store.record(make_feedback(2, 20, 200)).unwrap();
        store.record(make_feedback(3, 30, 80)).unwrap();

        let removed = store.prune_expired(ts(100));
        assert_eq!(removed, 2);
        let remaining: Vec<u64> = store.iter().map(|fb| fb.feedback_id().get()).collect();
        assert_eq!(remaining, vec![2]);
    }

    #[test]
    fn iter_for_procedure_at_filters_by_procedure_and_window() {
        let mut store = InMemoryProcedureFeedbackStore::new(8);
        let fb_a = make_feedback(1, 10, 100);
        let fb_b = ProcedureFeedback::new(
            FeedbackId::new(2).unwrap(),
            ProcedureId::new(11),
            None,
            StatsVersion::new(5),
            committed_completion(7),
            window(10, 100),
        )
        .unwrap();
        let fb_c = ProcedureFeedback::new(
            FeedbackId::new(3).unwrap(),
            ProcedureId::new(99), // different procedure
            None,
            StatsVersion::new(5),
            committed_completion(7),
            window(10, 100),
        )
        .unwrap();
        let fb_expired = ProcedureFeedback::new(
            FeedbackId::new(4).unwrap(),
            ProcedureId::new(11),
            None,
            StatsVersion::new(5),
            committed_completion(7),
            window(10, 50),
        )
        .unwrap();
        store.record(fb_a).unwrap();
        store.record(fb_b).unwrap();
        store.record(fb_c).unwrap();
        store.record(fb_expired).unwrap();

        let visible = store.iter_for_procedure_at(ProcedureId::new(11), ts(60));
        let ids: Vec<u64> = visible.iter().map(|fb| fb.feedback_id().get()).collect();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn store_iteration_order_is_deterministic() {
        let mut a = InMemoryProcedureFeedbackStore::new(8);
        let mut b = InMemoryProcedureFeedbackStore::new(8);
        // Insert in different orders; final iteration must agree.
        a.record(make_feedback(1, 30, 100)).unwrap();
        a.record(make_feedback(2, 10, 100)).unwrap();
        a.record(make_feedback(3, 20, 100)).unwrap();
        b.record(make_feedback(2, 10, 100)).unwrap();
        b.record(make_feedback(3, 20, 100)).unwrap();
        b.record(make_feedback(1, 30, 100)).unwrap();

        let ids_a: Vec<u64> = a.iter().map(|fb| fb.feedback_id().get()).collect();
        let ids_b: Vec<u64> = b.iter().map(|fb| fb.feedback_id().get()).collect();
        assert_eq!(ids_a, ids_b);
        assert_eq!(ids_a, vec![2, 3, 1]);
    }
}
