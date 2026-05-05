//! Core feedback record types: completion status, evidence, feedback identity,
//! the [`ProcedureFeedback`] record, and its error taxonomy.

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
        if value == 0 { None } else { Some(Self(value)) }
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
    pub(super) feedback_id: FeedbackId,
    pub(super) procedure_id: ProcedureId,
    /// Optional plan-cache key digest (see
    /// `crate::plan_cache::PlanCacheKey::digest`).  When absent the
    /// observation could not be attributed to a specific plan.
    pub(super) plan_cache_key_digest: Option<[u8; 32]>,
    pub(super) stats_version: StatsVersion,
    pub(super) completion: CompletionEvidence,
    pub(super) observed_window: ValidityWindow,
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
