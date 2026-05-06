use std::collections::BTreeMap;

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, EngineTimestamp, InvocationId, ProcedureId,
};

use crate::{
    InMemoryProcedureFeedbackStore, ProcedureFeedback, ProcedureFeedbackStore, QualifiedName,
    RecordOutcome,
};

use super::{
    decision::InvocationDecisionRecord, entry::ProcedureStoreEntry,
    registration::ProcedureRegistration,
};

/// Default per-procedure capacity for advisory feedback evidence.  The
/// store is bounded so observed-outcome feedback cannot grow without
/// limit; chosen as a small power-of-two sample window.  The limit is
/// part of the doctrine: advisory evidence is *sampled*, never
/// exhaustively retained.
pub const PROCEDURE_FEEDBACK_CAPACITY_PER_PROCEDURE: usize = 64;

/// Minimal in-memory Procedure Store.
///
/// Indexed by both [`ProcedureId`] and [`QualifiedName`] to enforce that
/// neither identity may collide. Re-registering the same exact entry is a
/// no-op (idempotent); any divergence is rejected.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProcedureStore {
    entries: BTreeMap<ProcedureId, ProcedureStoreEntry>,
    by_name: BTreeMap<QualifiedName, ProcedureId>,
    decisions: BTreeMap<ProcedureId, Vec<InvocationDecisionRecord>>,
    feedback: BTreeMap<ProcedureId, InMemoryProcedureFeedbackStore>,
}

impl ProcedureStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Register an entry. Returns [`ProcedureRegistration::Inserted`] for a
    /// new procedure or [`ProcedureRegistration::AlreadyRegistered`] if the
    /// exact same entry was previously registered. Any conflict (id collision
    /// with different metadata, name collision with a different id, or
    /// contract hash divergence for the same id) is rejected.
    pub fn register(
        &mut self,
        entry: ProcedureStoreEntry,
    ) -> AndromedaResult<ProcedureRegistration> {
        entry.validate()?;

        if let Some(existing) = self.entries.get(&entry.procedure_id) {
            if existing == &entry {
                return Ok(ProcedureRegistration::AlreadyRegistered);
            }
            if existing.binding.contract_hash != entry.binding.contract_hash {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure store rejects contract hash divergence for existing procedure id",
                ));
            }
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure store rejects re-registration with conflicting metadata",
            ));
        }

        if let Some(existing_id) = self.by_name.get(&entry.name)
            && *existing_id != entry.procedure_id
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure store rejects qualified name collision across procedure ids",
            ));
        }

        self.by_name.insert(entry.name.clone(), entry.procedure_id);
        self.entries.insert(entry.procedure_id, entry);
        Ok(ProcedureRegistration::Inserted)
    }

    pub fn get(&self, procedure_id: ProcedureId) -> Option<&ProcedureStoreEntry> {
        self.entries.get(&procedure_id)
    }

    pub fn lookup_by_name(&self, name: &QualifiedName) -> Option<&ProcedureStoreEntry> {
        let id = self.by_name.get(name)?;
        self.entries.get(id)
    }

    pub fn entries(&self) -> impl Iterator<Item = &ProcedureStoreEntry> {
        self.entries.values()
    }

    /// Attach decision evidence to an existing procedure. The contract hash
    /// and catalog version on the record must match the registered binding,
    /// preventing evidence from being recorded against stale or mismatched
    /// contract identities.
    pub fn attach_invocation_decision(
        &mut self,
        record: InvocationDecisionRecord,
    ) -> AndromedaResult<()> {
        record.validate()?;
        let entry = self.entries.get(&record.procedure_id).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store cannot attach decision evidence for unknown procedure id",
            )
        })?;

        if entry.binding.contract_hash != record.contract_hash {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store decision evidence contract hash does not match registered binding",
            ));
        }
        if entry.binding.catalog_version != record.catalog_version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store decision evidence catalog version does not match registered binding",
            ));
        }

        self.decisions
            .entry(record.procedure_id)
            .or_default()
            .push(record);
        Ok(())
    }

    pub fn invocation_decisions_for(
        &self,
        procedure_id: ProcedureId,
    ) -> &[InvocationDecisionRecord] {
        self.decisions
            .get(&procedure_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn invocation_decisions_for_invocation(
        &self,
        invocation_id: InvocationId,
    ) -> impl Iterator<Item = &InvocationDecisionRecord> {
        self.decisions
            .values()
            .flat_map(|records| records.iter())
            .filter(move |record| record.invocation_id == invocation_id)
    }

    pub fn total_recorded_decisions(&self) -> usize {
        self.decisions.values().map(Vec::len).sum()
    }

    /// Attach **advisory, non-authoritative** observed-outcome feedback
    /// to a registered procedure.
    ///
    /// Doctrine:
    /// - The feedback is evidence-only.
    ///   [`ProcedureFeedback::is_authoritative`] is hard-wired to
    ///   `false`; this method does not promote it.
    /// - The feedback's `procedure_id` must match a registered entry.
    /// - The feedback's `stats_version` must match the registered
    ///   binding's `stats_version`.  Feedback gathered against a
    ///   different stats snapshot is rejected so it cannot be silently
    ///   re-applied to the wrong snapshot.
    /// - Recording the same `feedback_id` with the same digest is an
    ///   idempotent [`RecordOutcome::Duplicate`].  Recording the same
    ///   `feedback_id` with a different digest is rejected as a
    ///   conflict — no silent overwrite.
    /// - Per-procedure retention is bounded by
    ///   [`PROCEDURE_FEEDBACK_CAPACITY_PER_PROCEDURE`].  Eviction is
    ///   deterministic (lowest `(issued_at, feedback_id)` first).
    pub fn attach_procedure_feedback(
        &mut self,
        feedback: ProcedureFeedback,
    ) -> AndromedaResult<RecordOutcome> {
        let entry = self.entries.get(&feedback.procedure_id()).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store cannot attach feedback for unknown procedure id",
            )
        })?;

        if entry.binding.stats_version != feedback.stats_version() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store feedback stats version does not match registered binding",
            ));
        }

        let bucket = self
            .feedback
            .entry(feedback.procedure_id())
            .or_insert_with(|| {
                InMemoryProcedureFeedbackStore::new(PROCEDURE_FEEDBACK_CAPACITY_PER_PROCEDURE)
            });

        bucket
            .record(feedback)
            .map_err(|err| AndromedaError::new(AndromedaErrorKind::Contract, format!("{err}")))
    }

    /// Read-only view of advisory feedback recorded for a procedure
    /// that is non-expired at `now`.  Returns an empty vector when no
    /// feedback is present.
    pub fn procedure_feedback_for_at(
        &self,
        procedure_id: ProcedureId,
        now: EngineTimestamp,
    ) -> Vec<ProcedureFeedback> {
        match self.feedback.get(&procedure_id) {
            Some(bucket) => bucket.iter_for_procedure_at(procedure_id, now),
            None => Vec::new(),
        }
    }

    /// Drop expired feedback across every procedure.  Returns the total
    /// number of records removed.
    pub fn prune_expired_procedure_feedback(&mut self, now: EngineTimestamp) -> usize {
        self.feedback
            .values_mut()
            .map(|bucket| bucket.prune_expired(now))
            .sum()
    }

    /// Total stored feedback records across every procedure.
    pub fn total_procedure_feedback(&self) -> usize {
        self.feedback.values().map(|bucket| bucket.len()).sum()
    }
}
