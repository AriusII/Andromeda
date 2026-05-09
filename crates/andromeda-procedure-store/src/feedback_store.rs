use andromeda_time::EngineTimestamp;
use andromeda_types::ProcedureId;

use crate::{FeedbackId, ProcedureFeedback};

/// Outcome of recording a feedback record into a store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOutcome {
    /// The record was newly stored.
    Stored,
    /// A byte-identical record already exists.
    Duplicate,
    /// The lowest-ranked existing record was evicted before storing the new one.
    StoredAfterEviction,
}

/// Closed set of reasons a feedback record may be rejected at store time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureFeedbackStoreError {
    /// A record with the same id but a different digest already exists.
    Conflict { feedback_id: FeedbackId },
    /// The store was constructed with capacity 0.
    ZeroCapacity,
}

impl core::fmt::Display for ProcedureFeedbackStoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Conflict { feedback_id } => write!(
                f,
                "ProcedureFeedback conflict: id={} already present with a different digest",
                feedback_id.get()
            ),
            Self::ZeroCapacity => f.write_str("ProcedureFeedback store has zero capacity"),
        }
    }
}

/// Narrow store surface for advisory feedback.
pub trait ProcedureFeedbackStore {
    fn record(
        &mut self,
        feedback: ProcedureFeedback,
    ) -> Result<RecordOutcome, ProcedureFeedbackStoreError>;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return non-expired feedback for `procedure_id` at `now`.
    fn iter_for_procedure_at(
        &self,
        procedure_id: ProcedureId,
        now: EngineTimestamp,
    ) -> Vec<ProcedureFeedback>;

    /// Drop every record whose window has expired by `now`.
    fn prune_expired(&mut self, now: EngineTimestamp) -> usize;
}

/// Bounded in-memory implementation with deterministic ordering and eviction.
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

    /// Read-only view in deterministic `(issued_at, feedback_id)` order.
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
